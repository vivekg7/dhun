package library

import (
	"encoding/binary"
	"io"
	"os"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"unicode/utf16"
)

// Chapter is one chapter inside a podcast episode or an audiobook file
// (docs/plans/031_podcasts_and_audiobooks.md).
type Chapter struct {
	StartMS int64  `json:"startMs"`
	Title   string `json:"title"`
}

// xiphChapter is the chapter extension to Vorbis comments, which Ogg Opus
// episodes carry: CHAPTER001=00:01:23.456, CHAPTER001NAME=Title.
var xiphChapter = regexp.MustCompile(`^CHAPTER(\d{3})$`)

func xiphChapters(m map[string][]string) []Chapter {
	var out []Chapter
	for k, v := range m {
		sub := xiphChapter.FindStringSubmatch(k)
		if sub == nil || len(v) == 0 {
			continue
		}
		start, ok := clockMS(v[0])
		if !ok {
			continue
		}
		title := ""
		if name := m[k+"NAME"]; len(name) > 0 {
			title = strings.TrimSpace(name[0])
		}
		out = append(out, Chapter{StartMS: start, Title: title})
	}
	return sortChapters(out)
}

// clockMS parses HH:MM:SS(.fff), MM:SS(.fff) or SS(.fff) into milliseconds.
func clockMS(s string) (int64, bool) {
	s = strings.TrimSpace(strings.Replace(s, ",", ".", 1))
	parts := strings.Split(s, ":")
	if len(parts) > 3 {
		return 0, false
	}
	var ms float64
	for _, p := range parts {
		v, err := strconv.ParseFloat(p, 64)
		if err != nil || v < 0 {
			return 0, false
		}
		ms = ms*60 + v
	}
	return int64(ms*1000 + 0.5), true
}

func sortChapters(c []Chapter) []Chapter {
	slices.SortStableFunc(c, func(a, b Chapter) int { return int(a.StartMS - b.StartMS) })
	return c
}

// id3Chapters reads the CHAP frames of an MP3's ID3v2 tag (ID3v2 chapter
// addendum), which taglib reads but its WebAssembly build does not return.
// It walks the frame headers and reads only the CHAP frames, so a large
// cover in the tag is skipped, not loaded.
func id3Chapters(abs string) []Chapter {
	f, err := os.Open(abs)
	if err != nil {
		return nil
	}
	defer f.Close()
	var h [10]byte
	if _, err := io.ReadFull(f, h[:]); err != nil || string(h[:3]) != "ID3" {
		return nil
	}
	major, flags := h[3], h[5]
	if major < 3 || major > 4 || flags&0x80 != 0 {
		return nil // ID3v2.2 has no CHAP; unsynchronised tags are rare enough to skip
	}
	end := 10 + int64(syncsafe(h[6:10]))
	pos := int64(10)
	if flags&0x40 != 0 { // extended header
		var b [4]byte
		if _, err := f.ReadAt(b[:], pos); err != nil {
			return nil
		}
		if major == 4 {
			pos += int64(syncsafe(b[:])) // counts itself
		} else {
			pos += 4 + int64(binary.BigEndian.Uint32(b[:]))
		}
	}
	var out []Chapter
	for pos+10 <= end {
		var fh [10]byte
		if _, err := f.ReadAt(fh[:], pos); err != nil || fh[0] == 0 {
			break // padding
		}
		size := int64(frameSize(fh[4:8], major))
		if size <= 0 || pos+10+size > end {
			break
		}
		if string(fh[:4]) == "CHAP" && size <= 1<<20 {
			b := make([]byte, size)
			if _, err := f.ReadAt(b, pos+10); err == nil {
				if c, ok := chapFrame(b, major); ok {
					out = append(out, c)
				}
			}
		}
		pos += 10 + size
	}
	return sortChapters(out)
}

// chapFrame parses a CHAP frame: an element ID, start and end times in ms,
// two byte offsets, then sub-frames, of which TIT2 is the title.
func chapFrame(b []byte, major byte) (Chapter, bool) {
	i := slices.Index(b, 0)
	if i < 0 || len(b) < i+1+16 {
		return Chapter{}, false
	}
	c := Chapter{StartMS: int64(binary.BigEndian.Uint32(b[i+1:]))}
	sub := b[i+17:]
	for len(sub) >= 10 {
		size := frameSize(sub[4:8], major)
		if size <= 0 || 10+size > len(sub) {
			break
		}
		if string(sub[:4]) == "TIT2" {
			c.Title = strings.TrimSpace(id3Text(sub[10 : 10+size]))
		}
		sub = sub[10+size:]
	}
	return c, true
}

func syncsafe(b []byte) int {
	return int(b[0]&0x7f)<<21 | int(b[1]&0x7f)<<14 | int(b[2]&0x7f)<<7 | int(b[3]&0x7f)
}

// frameSize is syncsafe in ID3v2.4 and a plain integer in 2.3.
func frameSize(b []byte, major byte) int {
	if major == 4 {
		return syncsafe(b)
	}
	return int(binary.BigEndian.Uint32(b))
}

// id3Text decodes a text frame: an encoding byte, then Latin-1, UTF-16
// with a byte-order mark, UTF-16BE or UTF-8.
func id3Text(b []byte) string {
	if len(b) == 0 {
		return ""
	}
	enc, b := b[0], b[1:]
	switch enc {
	case 0:
		r := make([]rune, 0, len(b))
		for _, c := range b {
			if c == 0 {
				break
			}
			r = append(r, rune(c))
		}
		return string(r)
	case 1, 2:
		big := enc == 2
		if len(b) >= 2 && (b[0] == 0xFF && b[1] == 0xFE || b[0] == 0xFE && b[1] == 0xFF) {
			big = b[0] == 0xFE
			b = b[2:]
		}
		u := make([]uint16, 0, len(b)/2)
		for i := 0; i+1 < len(b); i += 2 {
			v := binary.LittleEndian.Uint16(b[i:])
			if big {
				v = binary.BigEndian.Uint16(b[i:])
			}
			if v == 0 {
				break
			}
			u = append(u, v)
		}
		return string(utf16.Decode(u))
	default:
		s, _, _ := strings.Cut(string(b), "\x00")
		return s
	}
}
