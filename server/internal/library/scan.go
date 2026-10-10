// Package library keeps the songs and playlists tables in step with the
// files under the media root. Rules: docs/plans/005_storage_and_library_model.md.
package library

import (
	"context"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"os"
	"path"
	"path/filepath"
	"strings"
	"sync"

	"go.senan.xyz/taglib"

	"github.com/vivekg7/dhun/server/internal/store"
)

// Kinds of file, each under its own root (docs/plans/031_podcasts_and_audiobooks.md).
const (
	KindMusic     = "music"
	KindPodcast   = "podcast"
	KindAudiobook = "audiobook"
)

// Scanner reconciles the database with the media roots. One scan runs at a time.
type Scanner struct {
	DB   *sql.DB
	Root string // absolute media root (DHUN_MEDIA, mounted at /media)
	// The podcast and audiobook roots; "" or a folder that is not there is
	// not scanned.
	Podcasts, Audiobooks string
	Log                  *slog.Logger

	mu sync.Mutex
}

type root struct{ kind, dir string }

func (s *Scanner) roots() []root {
	return []root{{KindMusic, s.Root}, {KindPodcast, s.Podcasts}, {KindAudiobook, s.Audiobooks}}
}

// RootOf returns the folder a kind's paths are relative to.
func (s *Scanner) RootOf(kind string) string {
	for _, r := range s.roots() {
		if r.kind == kind {
			return r.dir
		}
	}
	return s.Root
}

// fileKey names a file across roots: a path is unique within its kind.
type fileKey struct{ kind, path string }

// Stats summarises one scan.
type Stats struct {
	Added, Updated, Moved, Missing, Unchanged, Failed int
	Regrouped                                         int // podcast and audiobook files whose book, show or place changed
	Playlists                                         int
}

func (s Stats) String() string {
	return fmt.Sprintf("added=%d updated=%d moved=%d missing=%d unchanged=%d failed=%d regrouped=%d playlists=%d",
		s.Added, s.Updated, s.Moved, s.Missing, s.Unchanged, s.Failed, s.Regrouped, s.Playlists)
}

type songRow struct {
	id         int64
	kind       string
	path       string
	size       int64
	mtimeNS    int64
	hash       string
	missing    bool
	folderArt  string
	lrc        bool
	art        string
	embedded   bool // embedded art: its key changes only when the file does
	transcript string
}

type seenFile struct {
	kind       string
	rel        string
	abs        string
	size       int64
	mtimeNS    int64
	folderArt  string
	folderKey  string // the folder cover's art key, if it has one
	lrc        bool
	transcript string

	existing *songRow // nil for a path the database has never seen
	read     bool     // tags and hash were read in this scan
	tags     tags
	hash     string
	art      string // the art key, for a file read in this scan
	err      error
}

// synologyDirs are Synology's own folders: thumbnails, the recycle bin, and
// snapshots, which would otherwise add a full copy of the library for every
// snapshot. Exact names only: an album may well be called "#1. Deer Hunter".
var synologyDirs = map[string]bool{"@eaDir": true, "@tmp": true, "#recycle": true, "#snapshot": true}

// skipDir reports whether a directory is outside the collection: the
// curation workflow's top-level _inbox/_meta/_trash (plan 004), hidden
// folders, and Synology's system folders.
func skipDir(rel, name string) bool {
	if !strings.Contains(rel, "/") && strings.HasPrefix(name, "_") {
		return true
	}
	return strings.HasPrefix(name, ".") || synologyDirs[name]
}

// coverNames are checked in order for an album's folder art.
var coverNames = []string{"cover.jpg", "cover.jpeg", "cover.png", "folder.jpg", "folder.jpeg", "folder.png", "front.jpg", "front.png", "albumart.jpg"}

// Scan walks the media root and applies every difference to the database in
// one transaction.
func (s *Scanner) Scan(ctx context.Context) (Stats, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	var st Stats

	known, err := s.loadSongs(ctx)
	if err != nil {
		return st, err
	}

	// A mistyped or unmounted path gives Docker an empty folder. Marking the
	// whole collection missing would be undone by the next good scan, but the
	// apps would see an empty library meanwhile; refuse instead. Each root
	// is checked on its own: an unmounted Podcasts leaves its episodes as
	// they were and the music is still scanned.
	var files []*seenFile
	skipped := map[string]bool{}
	for _, r := range s.roots() {
		var got []*seenFile
		err := s.walk(ctx, r, "", known, &got)
		if err != nil && (r.kind == KindMusic || ctx.Err() != nil) {
			return st, err
		}
		present := 0
		for _, k := range known {
			if k.kind == r.kind && !k.missing {
				present++
			}
		}
		if len(got) == 0 && present > 0 {
			msg := fmt.Sprintf("no audio files under %s, but %d %s files are known: is the folder mounted? Nothing was changed", r.dir, present, r.kind)
			if r.kind == KindMusic {
				return st, errors.New(msg)
			}
			s.Log.Error("scan: " + msg)
			skipped[r.kind] = true
		}
		files = append(files, got...)
	}

	seen := make(map[fileKey]bool, len(files))
	var toRead []*seenFile
	for _, f := range files {
		seen[fileKey{f.kind, f.rel}] = true
		r := f.existing
		// A song with embedded art scanned before art keys existed is read
		// once more, to hash its image.
		if r != nil && !r.missing && r.size == f.size && r.mtimeNS == f.mtimeNS && (r.art != "" || !r.embedded) {
			continue // unchanged audio; sibling files are compared in apply
		}
		f.read = true
		toRead = append(toRead, f)
	}
	readAll(ctx, toRead)

	tx, err := s.DB.BeginTx(ctx, nil)
	if err != nil {
		return st, err
	}
	defer tx.Rollback()
	version, err := store.NextLibraryVersion(ctx, tx)
	if err != nil {
		return st, err
	}

	// Candidates for "this new path is a file we already know": songs whose
	// path vanished in this scan, and songs already marked missing.
	// Within a kind: a file never moves between collections.
	byHash := map[string][]*songRow{}
	for _, r := range known {
		if !seen[fileKey{r.kind, r.path}] {
			byHash[r.kind+r.hash] = append(byHash[r.kind+r.hash], r)
		}
	}
	matched := map[int64]bool{}

	now := store.Now()
	for _, f := range files {
		switch {
		case f.err != nil:
			st.Failed++
			s.Log.Warn("scan: cannot read file", "path", f.rel, "err", f.err)
			if f.existing != nil {
				matched[f.existing.id] = true // keep it as it was rather than mark it missing
			}
		case !f.read:
			// Unchanged audio: only the sibling .lrc, transcript or cover may
			// have changed.
			r := f.existing
			art := f.folderKey
			if r.embedded {
				art = r.art
			}
			if r.folderArt != f.folderArt || r.lrc != f.lrc || r.art != art || r.transcript != f.transcript {
				if _, err := tx.ExecContext(ctx,
					`UPDATE songs SET folder_art = ?, lrc = ?, art = ?, transcript = ?, updated_at = ?, version = ? WHERE id = ?`,
					f.folderArt, f.lrc, art, f.transcript, now, version, r.id); err != nil {
					return st, err
				}
				st.Updated++
			} else {
				st.Unchanged++
			}
		case f.existing != nil:
			if err := updateSong(ctx, tx, f.existing.id, f, now, version); err != nil {
				return st, err
			}
			st.Updated++
		default:
			if r := takeMatch(byHash[f.kind+f.hash], matched); r != nil {
				if err := updateSong(ctx, tx, r.id, f, now, version); err != nil {
					return st, err
				}
				st.Moved++
			} else {
				if err := insertSong(ctx, tx, f, now, version); err != nil {
					return st, err
				}
				st.Added++
			}
		}
	}

	for _, r := range known {
		if !seen[fileKey{r.kind, r.path}] && !r.missing && !matched[r.id] && !skipped[r.kind] {
			if _, err := tx.ExecContext(ctx,
				`UPDATE songs SET missing_since = ?, version = ? WHERE id = ?`, now, version, r.id); err != nil {
				return st, err
			}
			st.Missing++
		}
	}

	for _, kind := range []string{KindPodcast, KindAudiobook} {
		if skipped[kind] {
			continue
		}
		n, err := regroup(ctx, tx, kind, version)
		if err != nil {
			return st, err
		}
		st.Regrouped += n
	}

	n, err := indexPlaylists(ctx, tx, s.Root, version)
	if err != nil {
		return st, err
	}
	st.Playlists = n

	if st.Added+st.Updated+st.Moved+st.Missing+st.Regrouped+st.Playlists == 0 {
		return st, nil // nothing changed: roll back, so the version is not burned
	}
	return st, tx.Commit()
}

func takeMatch(cands []*songRow, matched map[int64]bool) *songRow {
	// Prefer a song that vanished in this scan over one missing for longer.
	for _, missing := range []bool{false, true} {
		for _, r := range cands {
			if r.missing == missing && !matched[r.id] {
				matched[r.id] = true
				return r
			}
		}
	}
	return nil
}

func (s *Scanner) loadSongs(ctx context.Context) (map[fileKey]*songRow, error) {
	rows, err := s.DB.QueryContext(ctx,
		`SELECT id, kind, path, size, mtime_ns, hex(quick_hash), missing_since IS NOT NULL, folder_art, lrc, art, embedded_art, transcript FROM songs`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	known := map[fileKey]*songRow{}
	for rows.Next() {
		r := &songRow{}
		if err := rows.Scan(&r.id, &r.kind, &r.path, &r.size, &r.mtimeNS, &r.hash, &r.missing, &r.folderArt, &r.lrc, &r.art, &r.embedded, &r.transcript); err != nil {
			return nil, err
		}
		known[fileKey{r.kind, r.path}] = r
	}
	return known, rows.Err()
}

// walk lists one directory at a time, so each file's sibling cover, .lrc
// and transcript come from the same listing instead of extra stat calls.
func (s *Scanner) walk(ctx context.Context, r root, rel string, known map[fileKey]*songRow, out *[]*seenFile) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	if r.dir == "" {
		return nil
	}
	entries, err := os.ReadDir(filepath.Join(r.dir, filepath.FromSlash(rel)))
	if err != nil {
		if rel == "" {
			return err
		}
		s.Log.Warn("scan: cannot list folder", "path", rel, "err", err)
		return nil
	}
	lower := map[string]string{}
	for _, e := range entries {
		lower[strings.ToLower(e.Name())] = e.Name()
	}
	folderArt, folderKey := "", ""
	for _, c := range coverNames {
		if name, ok := lower[c]; ok {
			folderArt = name
			break
		}
	}
	names := make([]string, len(entries))
	for i, e := range entries {
		names[i] = e.Name()
	}
	if folderArt == "" && r.kind != KindMusic {
		// A book's folder often holds one image under the book's own name
		// ("The Three-Body Problem.jpg"): with nothing better, that is its
		// cover. Not for music, whose folders hold scans and booklets.
		for _, n := range names { // ReadDir sorts by name
			if ext := strings.ToLower(path.Ext(n)); ext == ".jpg" || ext == ".jpeg" || ext == ".png" {
				folderArt = n
				break
			}
		}
	}
	for _, e := range entries {
		if e.Name() == folderArt {
			// Not the bytes: that would read every cover on every scan.
			if info, err := e.Info(); err == nil {
				folderKey = artKey(fmt.Sprintf("%s\x00%d\x00%d", path.Join(rel, folderArt), info.Size(), info.ModTime().UnixNano()))
			}
		}
	}
	for _, e := range entries {
		childRel := path.Join(rel, e.Name())
		if e.IsDir() {
			if !skipDir(childRel, e.Name()) {
				if err := s.walk(ctx, r, childRel, known, out); err != nil {
					return err
				}
			}
			continue
		}
		if !e.Type().IsRegular() || !isAudio(e.Name()) {
			continue
		}
		info, err := e.Info()
		if err != nil {
			continue
		}
		base := strings.TrimSuffix(e.Name(), path.Ext(e.Name()))
		_, hasLrc := lower[strings.ToLower(base)+".lrc"]
		transcript := ""
		if r.kind != KindMusic {
			transcript = transcriptName(names, e.Name())
		}
		*out = append(*out, &seenFile{
			kind:       r.kind,
			transcript: transcript,
			rel:        childRel,
			abs:        filepath.Join(r.dir, filepath.FromSlash(childRel)),
			size:       info.Size(),
			mtimeNS:    info.ModTime().UnixNano(),
			folderArt:  folderArt,
			folderKey:  folderKey,
			lrc:        hasLrc,
			existing:   known[fileKey{r.kind, childRel}],
		})
	}
	return nil
}

// readAll reads tags and hashes with a few workers: tag parsing runs in a
// WebAssembly runtime and is CPU-bound, while hashing waits on the disk.
func readAll(ctx context.Context, files []*seenFile) {
	work := make(chan *seenFile)
	var wg sync.WaitGroup
	for range 4 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for f := range work {
				readOne(f)
			}
		}()
	}
	for _, f := range files {
		if ctx.Err() != nil {
			break
		}
		work <- f
	}
	close(work)
	wg.Wait()
}

// readOne hashes and tags one file. A panic in the tag reader (a malformed
// file can trip go-taglib's Go side) fails that file, not the whole server.
func readOne(f *seenFile) {
	defer func() {
		if r := recover(); r != nil {
			f.err = fmt.Errorf("tag reader panicked: %v", r)
		}
	}()
	if f.hash, f.err = hashHex(f.abs, f.size); f.err != nil {
		return
	}
	if f.tags, f.err = readTags(f.abs, f.rel, f.kind != KindMusic); f.err != nil {
		return
	}
	f.art = f.folderKey
	if f.tags.EmbeddedArt {
		// Embedded art wins over the folder's, as in Art. An image that
		// cannot be read still gets a key of its own, so it is not read
		// again on every scan.
		img, err := taglib.ReadImage(f.abs)
		if err != nil || len(img) == 0 {
			img = []byte(fmt.Sprintf("unreadable\x00%s\x00%d", f.rel, f.mtimeNS))
		}
		f.art = artKey(string(img))
	}
}

// artKey is a short hash: 64 bits tell apart the covers of one library.
func artKey(s string) string {
	h := sha256.Sum256([]byte(s))
	return hex.EncodeToString(h[:8])
}

func hashHex(abs string, size int64) (string, error) {
	h, err := quickHash(abs, size)
	return fmt.Sprintf("%X", h), err // upper-case, to match SQLite's hex()
}

func songArgs(f *seenFile) []any {
	t := f.tags
	artists, _ := json.Marshal(t.Artists)
	genres, _ := json.Marshal(t.Genres)
	chapters, _ := json.Marshal(t.Chapters)
	if t.Chapters == nil {
		chapters = []byte("[]")
	}
	return []any{
		f.kind, f.rel, f.size, f.mtimeNS, f.hash,
		t.Title, t.Artist, string(artists), t.Album, t.AlbumArtist, t.Composer, t.Genre, string(genres),
		t.Year, t.Track, t.Disc,
		t.DurationMS, t.Format, t.Codec, t.Bitrate, t.SampleRate, t.BitDepth, t.Channels,
		t.EmbeddedArt, t.EmbeddedLyrics, f.folderArt, f.lrc, f.art,
		t.Date, t.Notes, string(chapters), f.transcript,
	}
}

const songCols = `kind, path, size, mtime_ns, quick_hash,
	title, artist, artists, album, album_artist, composer, genre, genres,
	year, track, disc,
	duration_ms, format, codec, bitrate, sample_rate, bit_depth, channels,
	embedded_art, embedded_lyrics, folder_art, lrc, art,
	date, notes, chapters, transcript`

func insertSong(ctx context.Context, tx *sql.Tx, f *seenFile, now string, version int64) error {
	args := append(songArgs(f), now, now, version)
	_, err := tx.ExecContext(ctx, `INSERT INTO songs (`+songCols+`, added_at, updated_at, version)
		VALUES (?, ?, ?, ?, unhex(?)`+strings.Repeat(", ?", len(args)-5)+`)`, args...)
	return err
}

func updateSong(ctx context.Context, tx *sql.Tx, id int64, f *seenFile, now string, version int64) error {
	args := append(songArgs(f), now, version, id)
	_, err := tx.ExecContext(ctx, `UPDATE songs SET
		kind = ?, path = ?, size = ?, mtime_ns = ?, quick_hash = unhex(?),
		title = ?, artist = ?, artists = ?, album = ?, album_artist = ?, composer = ?, genre = ?, genres = ?,
		year = ?, track = ?, disc = ?,
		duration_ms = ?, format = ?, codec = ?, bitrate = ?, sample_rate = ?, bit_depth = ?, channels = ?,
		embedded_art = ?, embedded_lyrics = ?, folder_art = ?, lrc = ?, art = ?,
		date = ?, notes = ?, chapters = ?, transcript = ?,
		updated_at = ?, version = ?, missing_since = NULL
		WHERE id = ?`, args...)
	return err
}
