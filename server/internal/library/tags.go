package library

import (
	"crypto/sha256"
	"encoding/binary"
	"io"
	"os"
	"path"
	"regexp"
	"strconv"
	"strings"

	"go.senan.xyz/taglib"
)

// audioExts are the files the scanner indexes: what Media3 on Android and the
// macOS app can both play. The macOS app plays most through Core Audio, and
// WebM Opus saved under an .opus name through a reader of its own
// (docs/plans/024_macos_app.md).
var audioExts = map[string]bool{
	".mp3": true, ".m4a": true, ".m4b": true, ".aac": true, ".flac": true,
	".opus": true, ".ogg": true, ".oga": true, ".wav": true, ".aif": true, ".aiff": true,
}

func isAudio(name string) bool { return audioExts[strings.ToLower(path.Ext(name))] }

// tags is what the scanner stores about one file.
type tags struct {
	Title, Artist, Album, AlbumArtist, Composer, Genre string
	Artists, Genres                                    []string
	Year, Track, Disc                                  int
	DurationMS                                         int64
	Format, Codec                                      string
	Bitrate, SampleRate, BitDepth, Channels            int
	EmbeddedArt, EmbeddedLyrics                        bool

	// Podcasts and audiobooks only (docs/plans/031_podcasts_and_audiobooks.md).
	Date, Notes string
	Chapters    []Chapter
}

// readTags reads tags and audio properties. A file taglib cannot read still
// becomes a song (titled after its file name), so it stays visible and
// playable rather than silently missing from the library. spoken adds what
// only podcasts and audiobooks keep: the full date, notes and chapters.
func readTags(abs, rel string, spoken bool) (tags, error) {
	var t tags
	var m map[string][]string
	var perr, terr error
	if f, size, ok := openMatroska(abs); ok {
		// taglib picks its parser by extension and has no Matroska one:
		// WebM saved as .opus would read as untagged, with no duration.
		m = readMatroska(f, size, &t)
		f.Close()
	} else {
		var props taglib.Properties
		props, perr = taglib.ReadProperties(abs)
		m, terr = taglib.ReadTags(abs)
		if perr == nil {
			t.DurationMS = props.Length.Milliseconds()
			t.Format, t.Codec = props.Format, props.InnerCodec
			t.Bitrate, t.SampleRate = int(props.BitRate), int(props.SampleRate)
			t.BitDepth, t.Channels = int(props.BitDepth), int(props.Channels)
			t.EmbeddedArt = len(props.Images) > 0
		}
	}
	first := func(keys ...string) string {
		for _, k := range keys {
			if v := m[k]; len(v) > 0 && strings.TrimSpace(v[0]) != "" {
				return strings.TrimSpace(v[0])
			}
		}
		return ""
	}
	t.Title = first(taglib.Title)
	t.Artist = strings.Join(m[taglib.Artist], "; ")
	t.Album = first(taglib.Album)
	t.AlbumArtist = first(taglib.AlbumArtist)
	t.Composer = strings.Join(m[taglib.Composer], "; ")
	t.Genre = strings.Join(m[taglib.Genre], "; ")
	t.Artists = splitNames(m[taglib.Artist])
	t.Genres = splitNames(m[taglib.Genre])
	t.Year = leadingInt(first(taglib.Date, "ORIGINALDATE", "YEAR"))
	t.Track = leadingInt(first(taglib.TrackNumber))
	t.Disc = leadingInt(first(taglib.DiscNumber))
	// MP3 keeps lyrics in USLT, other formats in LYRICS.
	t.EmbeddedLyrics = first(taglib.Lyrics, "USLT", "UNSYNCEDLYRICS") != ""

	if t.Title == "" {
		t.Title = titleFromFileName(rel)
	}
	if spoken {
		t.Date = first(taglib.Date, "ORIGINALDATE", "YEAR")
		// ffmpeg writes an Ogg file's comment as DESCRIPTION.
		t.Notes = first(taglib.Comment, "DESCRIPTION")
		if len(t.Chapters) == 0 { // Matroska's came with its tags
			t.Chapters = xiphChapters(m)
		}
		if len(t.Chapters) == 0 && strings.EqualFold(path.Ext(rel), ".mp3") {
			t.Chapters = id3Chapters(abs)
		}
	} else {
		t.Chapters = nil
	}
	if perr != nil && terr != nil {
		return t, perr
	}
	return t, nil
}

// openMatroska opens abs if it is Matroska or WebM, by its first bytes.
func openMatroska(abs string) (*os.File, int64, bool) {
	f, err := os.Open(abs)
	if err != nil {
		return nil, 0, false
	}
	var b [4]byte
	info, err := f.Stat()
	if err == nil {
		_, err = io.ReadFull(f, b[:])
	}
	if err != nil || string(b[:]) != EBMLMagic {
		f.Close()
		return nil, 0, false
	}
	return f, info.Size(), true
}

// nameSeparators split multi-artist and multi-genre tags such as
// "A.R. Rahman, Shreya Ghoshal & Javed Ali". The raw tag is kept alongside,
// so a wrong split ("Simon & Garfunkel") never loses information.
// "/" is deliberately not a separator: it would split AC/DC.
var nameSeparators = regexp.MustCompile(`\s*(?:,|;|&|\bfeat\.|\bft\.|\bfeaturing\b)\s*`)

func splitNames(values []string) []string {
	out := []string{}
	seen := map[string]bool{}
	for _, v := range values {
		for _, part := range nameSeparators.Split(v, -1) {
			part = strings.TrimSpace(part)
			if part != "" && !seen[strings.ToLower(part)] {
				seen[strings.ToLower(part)] = true
				out = append(out, part)
			}
		}
	}
	return out
}

// leadingInt parses "2014-05-01" → 2014 and "3/12" → 3.
func leadingInt(s string) int {
	end := 0
	for end < len(s) && s[end] >= '0' && s[end] <= '9' {
		end++
	}
	n, _ := strconv.Atoi(s[:end])
	return n
}

// trackPrefix matches the library's "1-04 - " and the common "04 - " / "04. ".
var trackPrefix = regexp.MustCompile(`^\d{1,2}(?:-\d{1,3})?\s*(?:-|\.)\s+`)

func titleFromFileName(rel string) string {
	base := path.Base(rel)
	base = strings.TrimSuffix(base, path.Ext(base))
	return trackPrefix.ReplaceAllString(base, "")
}

// quickHash identifies a file's content cheaply: SHA-256 of its size plus
// the first and last 64 KB. Hashing whole files would read the entire
// collection from HDD on every scan; at this scale a collision between two
// different songs is not a realistic risk (plan 005).
func quickHash(abs string, size int64) ([]byte, error) {
	f, err := os.Open(abs)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	h := sha256.New()
	binary.Write(h, binary.LittleEndian, size)
	const chunk = 64 << 10
	if _, err := io.CopyN(h, f, chunk); err != nil && err != io.EOF {
		return nil, err
	}
	if size > 2*chunk {
		if _, err := f.Seek(-chunk, io.SeekEnd); err != nil {
			return nil, err
		}
	}
	// Small files: the rest of the file; large ones: the last 64 KB.
	if _, err := io.Copy(h, f); err != nil {
		return nil, err
	}
	return h.Sum(nil), nil
}
