package library

import (
	"encoding/binary"
	"io"
	"math"
	"math/bits"
	"strings"
)

// Matroska and WebM, read by a small EBML parser of our own. YouTube
// downloaders save WebM with Opus inside under an .opus name, and the TagLib
// build in use (go.senan.xyz/taglib v0.14.0) reads no Matroska under any
// name, so such files were indexed with no format and a duration of 0.
// Layout: RFC 8794 (EBML) and RFC 9559 (Matroska).

// EBMLMagic starts every Matroska and WebM file. The scanner and the stream
// endpoint decide by it, never by the file's name.
const EBMLMagic = "\x1a\x45\xdf\xa3"

const (
	idEBML, idDocType                              = 0x1A45DFA3, 0x4282
	idSegment, idCluster                           = 0x18538067, 0x1F43B675
	idSeekHead, idSeek, idSeekID, idSeekPosition   = 0x114D9B74, 0x4DBB, 0x53AB, 0x53AC
	idInfo, idTimestampScale, idDuration, idTitle  = 0x1549A966, 0x2AD7B1, 0x4489, 0x7BA9
	idTracks, idTrackEntry, idTrackType, idCodecID = 0x1654AE6B, 0xAE, 0x83, 0x86
	idAudio, idSamplingFrequency, idChannels       = 0xE1, 0xB5, 0x9F
	idTags, idTag, idSimpleTag, idTagName          = 0x1254C367, 0x7373, 0x67C8, 0x45A3
	idTagString                                    = 0x4487
)

// matroskaTagNames maps Matroska's tag names (as ffmpeg and yt-dlp write
// them) to the keys taglib returns, so readTags reads both alike.
var matroskaTagNames = map[string]string{
	"ALBUM_ARTIST": "ALBUMARTIST", "PART_NUMBER": "TRACKNUMBER", "DATE_RELEASED": "DATE",
}

// readMatroska fills t's audio properties and returns the file's tags. It
// reads what it can: a damaged or truncated file still gives what precedes
// the damage, and the caller falls back to the file name for the title.
func readMatroska(r io.ReaderAt, size int64, t *tags) map[string][]string {
	m := map[string][]string{}
	// elem reads the header of the element at pos: its ID, where its data
	// starts and its size, -1 when unknown. ok is false past the end.
	elem := func(pos int64) (id uint32, data, n int64, ok bool) {
		var b [12]byte
		got, _ := r.ReadAt(b[:], pos)
		id, n, hl := header(b[:got])
		return id, pos + int64(hl), n, hl > 0
	}
	// body reads an element's data. Info, Tracks, Tags and SeekHead are a
	// few KB; a larger one is damage, not something to allocate for.
	body := func(data, n int64) []byte {
		if n < 0 || n > 1<<20 || data+n > size {
			return nil
		}
		b := make([]byte, n)
		if _, err := r.ReadAt(b, data); err != nil {
			return nil
		}
		return b
	}

	id, data, n, ok := elem(0)
	if !ok || id != idEBML {
		return m
	}
	children(body(data, n), func(id uint32, b []byte) {
		if id == idDocType {
			t.Format = string(b) // "webm" or "matroska"
		}
	})
	id, seg, segSize, ok := elem(data + n)
	if !ok || id != idSegment {
		return m
	}
	segEnd := size
	if segSize >= 0 && seg+segSize < size {
		segEnd = seg + segSize
	}

	title, scale, duration := "", 1_000_000.0, 0.0
	tagsAt, seekHead := int64(-1), false
	for pos := seg; pos < segEnd; {
		id, data, n, ok := elem(pos)
		if !ok {
			break
		}
		switch id {
		case idSeekHead:
			seekHead = true
			children(body(data, n), func(id uint32, b []byte) {
				if id != idSeek {
					return
				}
				var target uint32
				var at int64 = -1
				children(b, func(id uint32, b []byte) {
					switch id {
					case idSeekID:
						target = uint32(uintOf(b))
					case idSeekPosition:
						at = seg + int64(uintOf(b))
					}
				})
				if target == idTags {
					tagsAt = at
				}
			})
		case idInfo:
			children(body(data, n), func(id uint32, b []byte) {
				switch id {
				case idTimestampScale:
					scale = float64(uintOf(b))
				case idDuration:
					duration = floatOf(b)
				case idTitle:
					title = strings.TrimSpace(string(b))
				}
			})
		case idTracks:
			children(body(data, n), func(id uint32, b []byte) {
				if id == idTrackEntry && t.Codec == "" {
					readAudioTrack(b, t)
				}
			})
		case idTags:
			children(body(data, n), func(id uint32, b []byte) {
				if id != idTag {
					return
				}
				children(b, func(id uint32, b []byte) {
					if id == idSimpleTag {
						addSimpleTag(b, m)
					}
				})
			})
		case idCluster:
			// The audio. Tags usually come before it; when the SeekHead says
			// they come after, jump there instead of reading past every
			// cluster. With a SeekHead that lists no Tags, there are none.
			if tagsAt > pos {
				pos, tagsAt = tagsAt, -1
				continue
			}
			if seekHead {
				pos = segEnd
				continue
			}
		}
		if n < 0 {
			break // unknown size: only a live recording's clusters, nothing to skip to
		}
		pos = data + n
	}

	t.DurationMS = int64(duration * scale / 1e6)
	if t.DurationMS > 0 {
		t.Bitrate = int(size * 8 / t.DurationMS) // bits per ms = kbit/s, as taglib reports
	}
	if title != "" && len(m["TITLE"]) == 0 {
		m["TITLE"] = []string{title} // ffmpeg writes the title here, not as a tag
	}
	return m
}

// readAudioTrack fills the codec and properties from a TrackEntry, if it is
// the audio track. Not the bit depth: muxers write 16 for Opus, which has
// none, and taglib reports 0 for Ogg Opus.
func readAudioTrack(b []byte, t *tags) {
	var audio bool
	var codec string
	var props []byte
	children(b, func(id uint32, b []byte) {
		switch id {
		case idTrackType:
			audio = uintOf(b) == 2
		case idCodecID:
			codec = string(b)
		case idAudio:
			props = b
		}
	})
	if !audio {
		return
	}
	// "A_OPUS" → "opus", "A_AAC/MPEG4/LC" → "aac", as taglib names codecs.
	codec, _, _ = strings.Cut(strings.TrimPrefix(codec, "A_"), "/")
	t.Codec = strings.ToLower(codec)
	children(props, func(id uint32, b []byte) {
		switch id {
		case idSamplingFrequency:
			t.SampleRate = int(floatOf(b))
		case idChannels:
			t.Channels = int(uintOf(b))
		}
	})
}

func addSimpleTag(b []byte, m map[string][]string) {
	var name, value string
	children(b, func(id uint32, b []byte) {
		switch id {
		case idTagName:
			name = strings.ToUpper(string(b))
		case idTagString:
			value = string(b)
		}
	})
	if k, ok := matroskaTagNames[name]; ok {
		name = k
	}
	if name != "" && value != "" {
		m[name] = append(m[name], value)
	}
}

// children calls fn for each element directly inside b, stopping at the
// first that runs past b's end.
func children(b []byte, fn func(id uint32, data []byte)) {
	for len(b) > 0 {
		id, n, hl := header(b)
		if hl == 0 || n < 0 || n > int64(len(b)-hl) {
			return
		}
		fn(id, b[hl:hl+int(n)])
		b = b[hl+int(n):]
	}
}

// header parses an element header: its ID, its data size (-1 when unknown)
// and the header's length, 0 when b holds no valid header.
func header(b []byte) (id uint32, n int64, hl int) {
	v, il := vint(b, true)
	if il == 0 || il > 4 {
		return 0, 0, 0
	}
	size, sl := vint(b[il:], false)
	if sl == 0 {
		return 0, 0, 0
	}
	n = int64(size)
	if size == 1<<(7*sl)-1 {
		n = -1 // all ones: unknown size
	}
	return uint32(v), n, il + sl
}

// vint reads an EBML variable-length integer. An element ID keeps its length
// marker; a size drops it. l is 0 when b is too short or invalid.
func vint(b []byte, keepMarker bool) (v uint64, l int) {
	if len(b) == 0 || b[0] == 0 {
		return 0, 0
	}
	l = bits.LeadingZeros8(b[0]) + 1
	if len(b) < l {
		return 0, 0
	}
	v = uint64(b[0])
	if !keepMarker {
		v &= 0xFF >> l
	}
	for _, c := range b[1:l] {
		v = v<<8 | uint64(c)
	}
	return v, l
}

func uintOf(b []byte) uint64 {
	var v uint64
	for _, c := range b {
		v = v<<8 | uint64(c)
	}
	return v
}

func floatOf(b []byte) float64 {
	switch len(b) {
	case 4:
		return float64(math.Float32frombits(binary.BigEndian.Uint32(b)))
	case 8:
		return math.Float64frombits(binary.BigEndian.Uint64(b))
	}
	return 0
}
