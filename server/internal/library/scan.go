// Package library keeps the songs and playlists tables in step with the
// files under the media root. Rules: docs/plans/005_storage_and_library_model.md.
package library

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"log/slog"
	"os"
	"path"
	"path/filepath"
	"strings"
	"sync"

	"github.com/vivekg7/dhun/server/internal/store"
)

// Scanner reconciles the database with the media root. One scan runs at a time.
type Scanner struct {
	DB   *sql.DB
	Root string // absolute media root (DHUN_MEDIA, mounted at /media)
	Log  *slog.Logger

	mu sync.Mutex
}

// Stats summarises one scan.
type Stats struct {
	Added, Updated, Moved, Missing, Unchanged, Failed int
	Playlists                                         int
}

func (s Stats) String() string {
	return fmt.Sprintf("added=%d updated=%d moved=%d missing=%d unchanged=%d failed=%d playlists=%d",
		s.Added, s.Updated, s.Moved, s.Missing, s.Unchanged, s.Failed, s.Playlists)
}

type songRow struct {
	id        int64
	path      string
	size      int64
	mtimeNS   int64
	hash      string
	missing   bool
	folderArt string
	lrc       bool
}

type seenFile struct {
	rel       string
	abs       string
	size      int64
	mtimeNS   int64
	folderArt string
	lrc       bool

	existing *songRow // nil for a path the database has never seen
	read     bool     // tags and hash were read in this scan
	tags     tags
	hash     string
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

	var files []*seenFile
	if err := s.walk(ctx, "", known, &files); err != nil {
		return st, err
	}
	// A mistyped or unmounted path gives Docker an empty folder. Marking the
	// whole collection missing would be undone by the next good scan, but the
	// apps would see an empty library meanwhile; refuse instead.
	present := 0
	for _, r := range known {
		if !r.missing {
			present++
		}
	}
	if len(files) == 0 && present > 0 {
		return st, fmt.Errorf("no audio files under %s, but %d songs are known: is the Music folder mounted? Nothing was changed", s.Root, present)
	}

	seen := make(map[string]bool, len(files))
	var toRead []*seenFile
	for _, f := range files {
		seen[f.rel] = true
		r := f.existing
		if r != nil && !r.missing && r.size == f.size && r.mtimeNS == f.mtimeNS {
			continue // unchanged audio; sibling flags are compared in apply
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
	byHash := map[string][]*songRow{}
	for _, r := range known {
		if !seen[r.path] {
			byHash[r.hash] = append(byHash[r.hash], r)
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
			// Unchanged audio: only the sibling .lrc or cover may have changed.
			r := f.existing
			if r.folderArt != f.folderArt || r.lrc != f.lrc {
				if _, err := tx.ExecContext(ctx,
					`UPDATE songs SET folder_art = ?, lrc = ?, updated_at = ?, version = ? WHERE id = ?`,
					f.folderArt, f.lrc, now, version, r.id); err != nil {
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
			if r := takeMatch(byHash[f.hash], matched); r != nil {
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
		if !seen[r.path] && !r.missing && !matched[r.id] {
			if _, err := tx.ExecContext(ctx,
				`UPDATE songs SET missing_since = ?, version = ? WHERE id = ?`, now, version, r.id); err != nil {
				return st, err
			}
			st.Missing++
		}
	}

	n, err := indexPlaylists(ctx, tx, s.Root, version)
	if err != nil {
		return st, err
	}
	st.Playlists = n

	if st.Added+st.Updated+st.Moved+st.Missing+st.Playlists == 0 {
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

func (s *Scanner) loadSongs(ctx context.Context) (map[string]*songRow, error) {
	rows, err := s.DB.QueryContext(ctx,
		`SELECT id, path, size, mtime_ns, hex(quick_hash), missing_since IS NOT NULL, folder_art, lrc FROM songs`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	known := map[string]*songRow{}
	for rows.Next() {
		r := &songRow{}
		if err := rows.Scan(&r.id, &r.path, &r.size, &r.mtimeNS, &r.hash, &r.missing, &r.folderArt, &r.lrc); err != nil {
			return nil, err
		}
		known[r.path] = r
	}
	return known, rows.Err()
}

// walk lists one directory at a time, so each file's sibling cover and .lrc
// come from the same listing instead of extra stat calls.
func (s *Scanner) walk(ctx context.Context, rel string, known map[string]*songRow, out *[]*seenFile) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	entries, err := os.ReadDir(filepath.Join(s.Root, filepath.FromSlash(rel)))
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
	folderArt := ""
	for _, c := range coverNames {
		if name, ok := lower[c]; ok {
			folderArt = name
			break
		}
	}
	for _, e := range entries {
		childRel := path.Join(rel, e.Name())
		if e.IsDir() {
			if !skipDir(childRel, e.Name()) {
				if err := s.walk(ctx, childRel, known, out); err != nil {
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
		*out = append(*out, &seenFile{
			rel:       childRel,
			abs:       filepath.Join(s.Root, filepath.FromSlash(childRel)),
			size:      info.Size(),
			mtimeNS:   info.ModTime().UnixNano(),
			folderArt: folderArt,
			lrc:       hasLrc,
			existing:  known[childRel],
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
	f.tags, f.err = readTags(f.abs, f.rel)
}

func hashHex(abs string, size int64) (string, error) {
	h, err := quickHash(abs, size)
	return fmt.Sprintf("%X", h), err // upper-case, to match SQLite's hex()
}

func songArgs(f *seenFile) []any {
	t := f.tags
	artists, _ := json.Marshal(t.Artists)
	genres, _ := json.Marshal(t.Genres)
	return []any{
		f.rel, f.size, f.mtimeNS, f.hash,
		t.Title, t.Artist, string(artists), t.Album, t.AlbumArtist, t.Composer, t.Genre, string(genres),
		t.Year, t.Track, t.Disc,
		t.DurationMS, t.Format, t.Codec, t.Bitrate, t.SampleRate, t.BitDepth, t.Channels,
		t.EmbeddedArt, t.EmbeddedLyrics, f.folderArt, f.lrc,
	}
}

const songCols = `path, size, mtime_ns, quick_hash,
	title, artist, artists, album, album_artist, composer, genre, genres,
	year, track, disc,
	duration_ms, format, codec, bitrate, sample_rate, bit_depth, channels,
	embedded_art, embedded_lyrics, folder_art, lrc`

func insertSong(ctx context.Context, tx *sql.Tx, f *seenFile, now string, version int64) error {
	args := append(songArgs(f), now, now, version)
	_, err := tx.ExecContext(ctx, `INSERT INTO songs (`+songCols+`, added_at, updated_at, version)
		VALUES (?, ?, ?, unhex(?), ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`, args...)
	return err
}

func updateSong(ctx context.Context, tx *sql.Tx, id int64, f *seenFile, now string, version int64) error {
	args := append(songArgs(f), now, version, id)
	_, err := tx.ExecContext(ctx, `UPDATE songs SET
		path = ?, size = ?, mtime_ns = ?, quick_hash = unhex(?),
		title = ?, artist = ?, artists = ?, album = ?, album_artist = ?, composer = ?, genre = ?, genres = ?,
		year = ?, track = ?, disc = ?,
		duration_ms = ?, format = ?, codec = ?, bitrate = ?, sample_rate = ?, bit_depth = ?, channels = ?,
		embedded_art = ?, embedded_lyrics = ?, folder_art = ?, lrc = ?,
		updated_at = ?, version = ?, missing_since = NULL
		WHERE id = ?`, args...)
	return err
}
