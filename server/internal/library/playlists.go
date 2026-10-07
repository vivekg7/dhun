package library

import (
	"bufio"
	"context"
	"database/sql"
	"fmt"
	"os"
	"path"
	"path/filepath"
	"strconv"
	"strings"

	"golang.org/x/text/unicode/norm"
)

// PlaylistsDir holds the .m3u8 files: shared ones at the top level, each
// user's under Playlists/<username>/ (docs/REQUIREMENTS.md).
const PlaylistsDir = "Playlists"

// Entry is one song line of an .m3u8 playlist.
type Entry struct {
	Path      string   // as written in the file, relative to the playlist's folder
	Title     string   // from #EXTINF, may be empty
	DurationS int      // from #EXTINF, -1 if absent
	Before    []string // other # lines between the previous entry and this one
}

// Playlist is a parsed .m3u8 file.
type Playlist struct {
	Name    string // #PLAYLIST: header, else the file name
	Entries []Entry
	Trailer []string // # lines after the last entry
}

// ParsePlaylist reads an .m3u/.m3u8 file. Other # lines before the first
// entry are the header, which the writer re-reads from the file; those
// between entries travel with the entry that follows them.
func ParsePlaylist(abs string) (Playlist, error) {
	f, err := os.Open(abs)
	if err != nil {
		return Playlist{}, err
	}
	defer f.Close()
	p := Playlist{Name: strings.TrimSuffix(filepath.Base(abs), filepath.Ext(abs))}
	pending := Entry{DurationS: -1}
	started := false // past the header
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 64<<10), 1<<20)
	for first := true; sc.Scan(); first = false {
		line := strings.TrimSpace(sc.Text())
		if first {
			line = strings.TrimPrefix(line, "\uFEFF") // UTF-8 BOM
		}
		switch {
		case line == "":
		case strings.HasPrefix(line, "#PLAYLIST:"):
			if name := strings.TrimSpace(strings.TrimPrefix(line, "#PLAYLIST:")); name != "" {
				p.Name = name
			}
		case strings.HasPrefix(line, "#EXTINF:"):
			started = true
			dur, title, _ := strings.Cut(strings.TrimPrefix(line, "#EXTINF:"), ",")
			if d, err := strconv.Atoi(strings.TrimSpace(dur)); err == nil {
				pending.DurationS = d
			}
			pending.Title = strings.TrimSpace(title)
		case line == "#EXTM3U":
		case strings.HasPrefix(line, "#"):
			if started {
				pending.Before = append(pending.Before, line)
			}
		default:
			started = true
			pending.Path = line
			p.Entries = append(p.Entries, pending)
			pending = Entry{DurationS: -1}
		}
	}
	p.Trailer = pending.Before
	return p, sc.Err()
}

// PathKey is how song paths are compared: in Unicode NFC. A file copied from
// a Mac is often named in decomposed form (é as e + combining accent) while a
// playlist names it composed; both are the same file to every player.
func PathKey(p string) string { return norm.NFC.String(p) }

// Resolve turns an entry's path into a media-root-relative song path, as a
// PathKey.
func Resolve(playlistRel, entry string) string {
	entry = strings.ReplaceAll(entry, `\`, "/")
	if path.IsAbs(entry) {
		return "" // absolute paths are from another machine; they cannot match
	}
	p := path.Clean(path.Join(path.Dir(playlistRel), entry))
	if p == ".." || strings.HasPrefix(p, "../") {
		return ""
	}
	return PathKey(p)
}

// indexPlaylists re-reads every playlist file whose size or mtime changed,
// tombstones the ones that disappeared, and returns how many changed.
func indexPlaylists(ctx context.Context, tx *sql.Tx, root string, version int64) (int, error) {
	type known struct {
		id            int64
		size, mtimeNS int64
		owner         sql.NullInt64
		deleted       bool
	}
	existing := map[string]known{}
	rows, err := tx.QueryContext(ctx, `SELECT id, path, size, mtime_ns, owner_id, deleted FROM playlists`)
	if err != nil {
		return 0, err
	}
	for rows.Next() {
		var k known
		var p string
		if err := rows.Scan(&k.id, &p, &k.size, &k.mtimeNS, &k.owner, &k.deleted); err != nil {
			rows.Close()
			return 0, err
		}
		existing[p] = k
	}
	rows.Close()

	users := map[string]int64{}
	urows, err := tx.QueryContext(ctx, `SELECT id, name FROM users`)
	if err != nil {
		return 0, err
	}
	for urows.Next() {
		var id int64
		var name string
		if err := urows.Scan(&id, &name); err != nil {
			urows.Close()
			return 0, err
		}
		users[strings.ToLower(name)] = id
	}
	urows.Close()

	songIDs := map[string]int64{}
	srows, err := tx.QueryContext(ctx, `SELECT id, path FROM songs`)
	if err != nil {
		return 0, err
	}
	for srows.Next() {
		var id int64
		var p string
		if err := srows.Scan(&id, &p); err != nil {
			srows.Close()
			return 0, err
		}
		songIDs[PathKey(p)] = id
	}
	srows.Close()

	// As with songs: a missing Playlists folder is a mount problem, not every
	// playlist deleted at once.
	if _, err := os.Stat(filepath.Join(root, PlaylistsDir)); err != nil {
		for _, k := range existing {
			if !k.deleted {
				return 0, fmt.Errorf("%s is missing but playlists are known: is it mounted? Nothing was changed", PlaylistsDir)
			}
		}
	}

	changed := 0
	seen := map[string]bool{}
	err = filepath.WalkDir(filepath.Join(root, PlaylistsDir), func(abs string, d os.DirEntry, err error) error {
		if err != nil {
			if os.IsNotExist(err) {
				return filepath.SkipDir
			}
			return err
		}
		if d.IsDir() || !isPlaylist(d.Name()) {
			return nil
		}
		relOS, _ := filepath.Rel(root, abs)
		rel := filepath.ToSlash(relOS)
		if IsSpecialFile(rel) {
			return nil // a nightly copy of a table, not a playlist of its own
		}
		info, err := d.Info()
		if err != nil {
			return nil
		}
		seen[rel] = true
		// Playlists/<user>/x.m3u8 belongs to that user; anything else is shared.
		// Recomputed every scan, so creating a user claims an existing folder.
		var owner sql.NullInt64
		if parts := strings.Split(rel, "/"); len(parts) > 2 {
			owner.Int64, owner.Valid = users[strings.ToLower(parts[1])]
		}
		k, ok := existing[rel]
		if ok && !k.deleted && k.size == info.Size() && k.mtimeNS == info.ModTime().UnixNano() {
			if k.owner != owner {
				if _, err := tx.ExecContext(ctx, `UPDATE playlists SET owner_id = ?, version = ? WHERE id = ?`, owner, version, k.id); err != nil {
					return err
				}
				changed++
			}
			return nil
		}
		pl, err := ParsePlaylist(abs)
		if err != nil {
			return nil // unreadable now; retried on the next scan
		}
		var existingID int64
		if ok {
			existingID = k.id
		}
		if _, err := storePlaylist(ctx, tx, rel, existingID, owner, pl, info, version, func(p string) (int64, bool) {
			id, ok := songIDs[p]
			return id, ok
		}); err != nil {
			return err
		}
		changed++
		return nil
	})
	if err != nil {
		return changed, err
	}
	for p, k := range existing {
		if !seen[p] && !k.deleted {
			if _, err := tx.ExecContext(ctx, `UPDATE playlists SET deleted = 1, version = ? WHERE id = ?`, version, k.id); err != nil {
				return changed, err
			}
			if _, err := tx.ExecContext(ctx, `DELETE FROM playlist_items WHERE playlist_id = ?`, k.id); err != nil {
				return changed, err
			}
			changed++
		}
	}
	return changed, nil
}

// storePlaylist writes a parsed playlist and its items into the index,
// updating row existingID or inserting a new one, and returns the row's ID.
func storePlaylist(ctx context.Context, tx *sql.Tx, rel string, existingID int64, owner sql.NullInt64,
	pl Playlist, info os.FileInfo, version int64, songAt func(path string) (int64, bool)) (int64, error) {
	id := existingID
	if id != 0 {
		if _, err := tx.ExecContext(ctx, `UPDATE playlists SET path = ?, owner_id = ?, name = ?, size = ?, mtime_ns = ?,
			deleted = 0, version = ? WHERE id = ?`, rel, owner, pl.Name, info.Size(), info.ModTime().UnixNano(), version, id); err != nil {
			return 0, err
		}
		if _, err := tx.ExecContext(ctx, `DELETE FROM playlist_items WHERE playlist_id = ?`, id); err != nil {
			return 0, err
		}
	} else if err := tx.QueryRowContext(ctx, `INSERT INTO playlists (path, owner_id, name, size, mtime_ns, version)
		VALUES (?, ?, ?, ?, ?, ?) RETURNING id`, rel, owner, pl.Name, info.Size(), info.ModTime().UnixNano(), version).Scan(&id); err != nil {
		return 0, err
	}
	for i, e := range pl.Entries {
		var songID any
		if sid, ok := songAt(Resolve(rel, e.Path)); ok {
			songID = sid
		}
		if _, err := tx.ExecContext(ctx, `INSERT INTO playlist_items (playlist_id, pos, song_id, raw_path, title, duration_s,
			lines_before) VALUES (?, ?, ?, ?, ?, ?, ?)`, id, i, songID, e.Path, e.Title, e.DurationS,
			strings.Join(e.Before, "\n")); err != nil {
			return 0, err
		}
	}
	return id, nil
}

func isPlaylist(name string) bool {
	ext := strings.ToLower(path.Ext(name))
	return ext == ".m3u8" || ext == ".m3u"
}

// RefreshPlaylist re-indexes the playlist file at rel if it changed on disk
// since it was indexed: someone edited it over SMB, or another player saved
// it. An edit through Dhun rebuilds the file from the index, so without this
// it would silently undo those changes until the next scan. next is called
// for a library version only when something changed.
func RefreshPlaylist(ctx context.Context, tx *sql.Tx, root, rel string, id int64, owner sql.NullInt64,
	next func() (int64, error)) error {
	abs := filepath.Join(root, filepath.FromSlash(rel))
	info, err := os.Stat(abs)
	if err != nil {
		return err
	}
	var size, mtimeNS int64
	if err := tx.QueryRowContext(ctx, `SELECT size, mtime_ns FROM playlists WHERE id = ?`, id).Scan(&size, &mtimeNS); err != nil {
		return err
	}
	if size == info.Size() && mtimeNS == info.ModTime().UnixNano() {
		return nil
	}
	pl, err := ParsePlaylist(abs)
	if err != nil {
		return err
	}
	version, err := next()
	if err != nil {
		return err
	}
	_, err = storePlaylist(ctx, tx, rel, id, owner, pl, info, version, songAtTx(ctx, tx))
	return err
}

// songAtTx looks songs up by PathKey. SQL cannot compare in NFC, so the
// paths are loaded once, on the first lookup (7,000 rows: a few milliseconds).
func songAtTx(ctx context.Context, tx *sql.Tx) func(string) (int64, bool) {
	var ids map[string]int64
	return func(p string) (int64, bool) {
		if ids == nil {
			ids = map[string]int64{}
			rows, err := tx.QueryContext(ctx, `SELECT id, path FROM songs`)
			if err != nil {
				return 0, false
			}
			defer rows.Close()
			for rows.Next() {
				var id int64
				var sp string
				if rows.Scan(&id, &sp) == nil {
					ids[PathKey(sp)] = id
				}
			}
		}
		id, ok := ids[p]
		return id, ok
	}
}
