package library

import (
	"bytes"
	"errors"
	"image"
	"image/jpeg"
	_ "image/png" // decode folder covers saved as PNG
	"os"
	"path"
	"path/filepath"
	"regexp"
	"strings"

	"go.senan.xyz/taglib"
	"golang.org/x/image/draw"
)

// ErrNone means the song has no art or lyrics.
var ErrNone = errors.New("none")

// SongFile is what the media helpers need to know about a song.
type SongFile struct {
	Root           string // its kind's root folder
	Path           string // relative to Root
	EmbeddedArt    bool
	EmbeddedLyrics bool
	FolderArt      string // sibling cover file name, if any
	Lrc            bool
	Transcript     string // sibling transcript file name, if any (plan 031)
}

func abs(root, rel string) string { return filepath.Join(root, filepath.FromSlash(rel)) }

// Decoding allocates width×height×4 bytes whatever the file size, so a
// 70-byte PNG claiming 16000×16000 pixels would take a gigabyte. Larger
// images are served unscaled, and two decodes at most run at once.
const maxArtPixels = 40_000_000

var decodeSlots = make(chan struct{}, 2)

// Art returns the song's cover: embedded art first, then the folder's cover
// file, the same order Musicolet uses. size > 0 scales the longest side down
// to size pixels and re-encodes as JPEG; the original is never upscaled.
func Art(f SongFile, size int) (data []byte, mimeType string, err error) {
	root := f.Root
	switch {
	case f.EmbeddedArt:
		data, err = taglib.ReadImage(abs(root, f.Path))
		if err == nil && len(data) == 0 {
			err = ErrNone
		}
	case f.FolderArt != "":
		data, err = os.ReadFile(abs(root, path.Join(path.Dir(f.Path), f.FolderArt)))
	default:
		return nil, "", ErrNone
	}
	if err != nil {
		return nil, "", err
	}
	if size <= 0 {
		return data, SniffImage(data), nil
	}
	cfg, _, err := image.DecodeConfig(bytes.NewReader(data))
	if err != nil || cfg.Width*cfg.Height > maxArtPixels || cfg.Width <= size && cfg.Height <= size {
		// Undecodable (e.g. WebP), too large to decode safely, or small
		// enough already: serve as-is.
		return data, SniffImage(data), nil
	}
	decodeSlots <- struct{}{}
	img, _, err := image.Decode(bytes.NewReader(data))
	<-decodeSlots
	if err != nil {
		return data, SniffImage(data), nil
	}
	b := img.Bounds()
	w, h := size, b.Dy()*size/b.Dx()
	if b.Dy() > b.Dx() {
		w, h = b.Dx()*size/b.Dy(), size
	}
	dst := image.NewRGBA(image.Rect(0, 0, max(w, 1), max(h, 1)))
	draw.CatmullRom.Scale(dst, dst.Bounds(), img, b, draw.Src, nil)
	var buf bytes.Buffer
	if err := jpeg.Encode(&buf, dst, &jpeg.Options{Quality: 85}); err != nil {
		return nil, "", err
	}
	return buf.Bytes(), "image/jpeg", nil
}

// SniffImage names an image type from its first bytes. Anything that is not
// PNG or WebP is called JPEG: art is only ever served as an image type, never
// as something a browser would run (see the nosniff header in the API).
func SniffImage(b []byte) string {
	switch {
	case bytes.HasPrefix(b, []byte("\x89PNG")):
		return "image/png"
	case bytes.HasPrefix(b, []byte("RIFF")) && len(b) > 12 && string(b[8:12]) == "WEBP":
		return "image/webp"
	default:
		return "image/jpeg"
	}
}

// Lyrics is a song's lyrics text, as stored; clients parse LRC themselves.
type Lyrics struct {
	Source string `json:"source"` // "lrc", "embedded", or an episode's "transcript" or "vtt"
	Synced bool   `json:"synced"` // has [mm:ss.xx] timestamps
	Text   string `json:"text"`
}

var lrcTimestamp = regexp.MustCompile(`(?m)^\s*\[\d{1,3}:\d{2}(?:[.:]\d{1,3})?\]`)

// ReadLyrics prefers the sibling .lrc file: the curation workflow fetches
// those and they are usually synced, while embedded lyrics often are not.
// An episode's transcript comes last, converted to LRC.
func ReadLyrics(f SongFile) (Lyrics, error) {
	root := f.Root
	if f.Lrc {
		base := strings.TrimSuffix(f.Path, path.Ext(f.Path))
		dir, err := os.ReadDir(abs(root, path.Dir(f.Path)))
		if err == nil {
			want := strings.ToLower(path.Base(base) + ".lrc")
			for _, e := range dir {
				if strings.ToLower(e.Name()) == want {
					if data, err := os.ReadFile(abs(root, path.Join(path.Dir(f.Path), e.Name()))); err == nil {
						return makeLyrics("lrc", string(data)), nil
					}
				}
			}
		}
	}
	if f.EmbeddedLyrics {
		m, err := taglib.ReadTags(abs(root, f.Path))
		if err != nil {
			return Lyrics{}, err
		}
		for _, k := range []string{taglib.Lyrics, "USLT", "UNSYNCEDLYRICS"} {
			if v := m[k]; len(v) > 0 && strings.TrimSpace(v[0]) != "" {
				return makeLyrics("embedded", v[0]), nil
			}
		}
	}
	if f.Transcript != "" {
		data, err := os.ReadFile(abs(root, path.Join(path.Dir(f.Path), f.Transcript)))
		if err != nil {
			return Lyrics{}, err
		}
		return transcriptLyrics(f.Transcript, string(data)), nil
	}
	return Lyrics{}, ErrNone
}

func makeLyrics(source, text string) Lyrics {
	text = strings.TrimPrefix(text, "\uFEFF")
	return Lyrics{Source: source, Synced: lrcTimestamp.MatchString(text), Text: text}
}
