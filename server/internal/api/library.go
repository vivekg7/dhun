package api

import (
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"os"
	"path"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/vivekg7/dhun/server/internal/library"
)

type songJSON struct {
	ID          int64           `json:"id"`
	Path        string          `json:"path"`
	Title       string          `json:"title"`
	Artist      string          `json:"artist,omitempty"`
	Artists     json.RawMessage `json:"artists"`
	Album       string          `json:"album,omitempty"`
	AlbumArtist string          `json:"albumArtist,omitempty"`
	Composer    string          `json:"composer,omitempty"`
	Genre       string          `json:"genre,omitempty"`
	Genres      json.RawMessage `json:"genres"`
	Year        int             `json:"year,omitempty"`
	Track       int             `json:"track,omitempty"`
	Disc        int             `json:"disc,omitempty"`
	DurationMS  int64           `json:"durationMs"`
	Format      string          `json:"format,omitempty"`
	Codec       string          `json:"codec,omitempty"`
	Bitrate     int             `json:"bitrate,omitempty"`
	SampleRate  int             `json:"sampleRate,omitempty"`
	BitDepth    int             `json:"bitDepth,omitempty"`
	Size        int64           `json:"size"`
	HasArt      bool            `json:"hasArt"`
	HasLyrics   bool            `json:"hasLyrics"`
	AddedAt     string          `json:"addedAt"`
	Missing     bool            `json:"missing,omitempty"`
}

type playlistJSON struct {
	ID      int64  `json:"id"`
	Deleted bool   `json:"deleted,omitempty"`
	Path    string `json:"path,omitempty"`
	Name    string `json:"name,omitempty"`
	Shared  bool   `json:"shared,omitempty"`
	// Song IDs in order; 0 marks an entry that resolves to no song (kept,
	// never dropped, so a fixed file brings it back).
	Songs []int64 `json:"songs,omitempty"`
}

// library returns every song and visible playlist changed since the client's
// cursor; the first call (since=0) returns everything. Clients keep the whole
// catalogue and browse it locally (plan 006).
func (s *Server) library(w http.ResponseWriter, r *http.Request, sess session) {
	since, _ := strconv.ParseInt(r.URL.Query().Get("since"), 10, 64)
	ctx := r.Context()

	// One read transaction, so the version and the rows agree even if a scan
	// commits halfway through this request.
	tx, err := s.DB.BeginTx(ctx, &sql.TxOptions{ReadOnly: true})
	if err != nil {
		s.fail(w, r, err)
		return
	}
	defer tx.Rollback()

	var version int64
	if err := tx.QueryRowContext(ctx, `SELECT value FROM meta WHERE key = 'library_version'`).Scan(&version); err != nil {
		s.fail(w, r, err)
		return
	}

	rows, err := tx.QueryContext(ctx, `SELECT id, path, title, artist, artists, album, album_artist, composer,
		genre, genres, year, track, disc, duration_ms, format, codec, bitrate, sample_rate, bit_depth, size,
		embedded_art OR folder_art != '', embedded_lyrics OR lrc, added_at, missing_since IS NOT NULL
		FROM songs WHERE version > ? ORDER BY id`, since)
	if err != nil {
		s.fail(w, r, err)
		return
	}
	songs := []songJSON{}
	for rows.Next() {
		var x songJSON
		var artists, genres string
		if err := rows.Scan(&x.ID, &x.Path, &x.Title, &x.Artist, &artists, &x.Album, &x.AlbumArtist, &x.Composer,
			&x.Genre, &genres, &x.Year, &x.Track, &x.Disc, &x.DurationMS, &x.Format, &x.Codec, &x.Bitrate,
			&x.SampleRate, &x.BitDepth, &x.Size, &x.HasArt, &x.HasLyrics, &x.AddedAt, &x.Missing); err != nil {
			rows.Close()
			s.fail(w, r, err)
			return
		}
		x.Artists, x.Genres = json.RawMessage(artists), json.RawMessage(genres)
		songs = append(songs, x)
	}
	rows.Close()

	playlists, err := s.changedPlaylists(r, tx, since, sess.UserID)
	if err != nil {
		s.fail(w, r, err)
		return
	}
	writeJSON(w, r, map[string]any{"version": version, "songs": songs, "playlists": playlists})
}

// changedPlaylists returns shared playlists and the caller's own; another
// user's playlists are private (REQUIREMENTS: per-user data stays private).
func (s *Server) changedPlaylists(r *http.Request, tx *sql.Tx, since, userID int64) ([]playlistJSON, error) {
	rows, err := tx.QueryContext(r.Context(), `SELECT id, path, name, owner_id IS NULL, deleted FROM playlists
		WHERE version > ? AND (owner_id IS NULL OR owner_id = ?) ORDER BY path`, since, userID)
	if err != nil {
		return nil, err
	}
	out := []playlistJSON{}
	for rows.Next() {
		var p playlistJSON
		if err := rows.Scan(&p.ID, &p.Path, &p.Name, &p.Shared, &p.Deleted); err != nil {
			rows.Close()
			return nil, err
		}
		if p.Deleted {
			p = playlistJSON{ID: p.ID, Deleted: true}
		}
		out = append(out, p)
	}
	rows.Close()
	for i := range out {
		if out[i].Deleted {
			continue
		}
		items, err := tx.QueryContext(r.Context(),
			`SELECT COALESCE(song_id, 0) FROM playlist_items WHERE playlist_id = ? ORDER BY pos`, out[i].ID)
		if err != nil {
			return nil, err
		}
		out[i].Songs = []int64{}
		for items.Next() {
			var id int64
			items.Scan(&id)
			out[i].Songs = append(out[i].Songs, id)
		}
		items.Close()
	}
	return out, nil
}

func (s *Server) songFile(r *http.Request) (library.SongFile, int64, error) {
	id, err := pathID(r)
	if err != nil {
		return library.SongFile{}, 0, err
	}
	var f library.SongFile
	var missing bool
	var version int64
	err = s.DB.QueryRowContext(r.Context(), `SELECT path, embedded_art, embedded_lyrics, folder_art, lrc,
		missing_since IS NOT NULL, version FROM songs WHERE id = ?`, id).
		Scan(&f.Path, &f.EmbeddedArt, &f.EmbeddedLyrics, &f.FolderArt, &f.Lrc, &missing, &version)
	if errors.Is(err, sql.ErrNoRows) || missing {
		return f, 0, errNotFound("song")
	}
	return f, version, err
}

var audioTypes = map[string]string{
	".mp3": "audio/mpeg", ".m4a": "audio/mp4", ".m4b": "audio/mp4", ".aac": "audio/aac",
	".flac": "audio/flac", ".opus": "audio/ogg", ".ogg": "audio/ogg", ".oga": "audio/ogg",
	".wav": "audio/wav", ".aif": "audio/aiff", ".aiff": "audio/aiff",
}

// stream serves the original file with Range support, so seeking works and
// downloads can resume (plan 006). Never transcoded.
func (s *Server) stream(w http.ResponseWriter, r *http.Request, _ session) {
	f, _, err := s.songFile(r)
	if err != nil {
		s.fail(w, r, err)
		return
	}
	file, err := os.Open(filepath.Join(s.Root, filepath.FromSlash(f.Path)))
	if err != nil {
		s.fail(w, r, errNotFound("song file"))
		return
	}
	defer file.Close()
	info, err := file.Stat()
	if err != nil {
		s.fail(w, r, err)
		return
	}
	if t, ok := audioTypes[strings.ToLower(path.Ext(f.Path))]; ok {
		w.Header().Set("Content-Type", t)
	}
	http.ServeContent(w, r, path.Base(f.Path), info.ModTime(), file)
}

// art serves cover art, scaled when ?size= is given, from a disk cache keyed
// by song and version: a re-tag or a new cover bumps the version.
func (s *Server) art(w http.ResponseWriter, r *http.Request, _ session) {
	f, version, err := s.songFile(r)
	if err != nil {
		s.fail(w, r, err)
		return
	}
	size, _ := strconv.Atoi(r.URL.Query().Get("size"))
	size = artSize(size)
	id := r.PathValue("id")
	etag := fmt.Sprintf(`"%s-%d-%d"`, id, version, size)
	w.Header().Set("ETag", etag)
	w.Header().Set("Cache-Control", "private, max-age=604800")
	if r.Header.Get("If-None-Match") == etag {
		w.WriteHeader(http.StatusNotModified)
		return
	}

	cacheDir := filepath.Join(s.DataDir, "cache", "art")
	cache := filepath.Join(cacheDir, fmt.Sprintf("%s-%d-%d", id, version, size))
	if data, err := os.ReadFile(cache); err == nil {
		w.Header().Set("Content-Type", library.SniffImage(data))
		w.Write(data)
		return
	}
	data, mime, err := library.Art(s.Root, f, size)
	if errors.Is(err, library.ErrNone) {
		s.fail(w, r, errNotFound("art"))
		return
	}
	if err != nil {
		s.fail(w, r, err)
		return
	}
	cacheArt(cacheDir, cache, id, version, data)
	w.Header().Set("Content-Type", mime)
	w.Write(data)
}

// artSize rounds a requested size up to one of a few, so the cache holds at
// most four scaled copies per cover. 0 means the original.
func artSize(n int) int {
	for _, s := range []int{128, 256, 512, 1024} {
		if n > 0 && n <= s {
			return s
		}
	}
	return 0
}

// cacheArt stores data best effort (a miss just recomputes): atomically, so a
// crash never leaves a truncated image to be served, and replacing the
// copies of the song's older versions.
func cacheArt(dir, file, id string, version int64, data []byte) {
	if os.MkdirAll(dir, 0o755) != nil {
		return
	}
	old, _ := filepath.Glob(filepath.Join(dir, id+"-*"))
	for _, f := range old {
		if !strings.HasPrefix(filepath.Base(f), fmt.Sprintf("%s-%d-", id, version)) {
			os.Remove(f)
		}
	}
	tmp, err := os.CreateTemp(dir, ".tmp-*")
	if err != nil {
		return
	}
	_, err = tmp.Write(data)
	if cerr := tmp.Close(); err == nil && cerr == nil {
		err = os.Rename(tmp.Name(), file)
	}
	if err != nil {
		os.Remove(tmp.Name())
	}
}

func (s *Server) lyrics(w http.ResponseWriter, r *http.Request, _ session) {
	f, _, err := s.songFile(r)
	if err != nil {
		s.fail(w, r, err)
		return
	}
	l, err := library.ReadLyrics(s.Root, f)
	if errors.Is(err, library.ErrNone) {
		s.fail(w, r, errNotFound("lyrics"))
		return
	}
	if err != nil {
		s.fail(w, r, err)
		return
	}
	w.Header().Set("Cache-Control", "private, max-age=3600")
	writeJSON(w, r, l)
}
