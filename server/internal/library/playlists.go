package library

import (
	"bufio"
	"context"
	"database/sql"
	"os"
	"path"
	"path/filepath"
	"strconv"
	"strings"
)

// PlaylistsDir holds the .m3u8 files: shared ones at the top level, each
// user's under Playlists/<username>/ (docs/REQUIREMENTS.md).
const PlaylistsDir = "Playlists"

// Entry is one song line of an .m3u8 playlist.
type Entry struct {
	Path      string // as written in the file, relative to the playlist's folder
	Title     string // from #EXTINF, may be empty
	DurationS int    // from #EXTINF, -1 if absent
}

// Playlist is a parsed .m3u8 file.
type Playlist struct {
	Name    string // #PLAYLIST: header, else the file name
	Entries []Entry
}

// ParsePlaylist reads an .m3u/.m3u8 file. Unknown # lines are ignored here;
// the writer preserves them when it edits a file.
func ParsePlaylist(abs string) (Playlist, error) {
	f, err := os.Open(abs)
	if err != nil {
		return Playlist{}, err
	}
	defer f.Close()
	p := Playlist{Name: strings.TrimSuffix(filepath.Base(abs), filepath.Ext(abs))}
	pending := Entry{DurationS: -1}
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
			dur, title, _ := strings.Cut(strings.TrimPrefix(line, "#EXTINF:"), ",")
			if d, err := strconv.Atoi(strings.TrimSpace(dur)); err == nil {
				pending.DurationS = d
			}
			pending.Title = strings.TrimSpace(title)
		case strings.HasPrefix(line, "#"):
		default:
			pending.Path = line
			p.Entries = append(p.Entries, pending)
			pending = Entry{DurationS: -1}
		}
	}
	return p, sc.Err()
}

// resolve turns an entry's path into a media-root-relative song path.
func resolve(playlistRel, entry string) string {
	entry = strings.ReplaceAll(entry, `\`, "/")
	if path.IsAbs(entry) {
		return "" // absolute paths are from another machine; they cannot match
	}
	p := path.Clean(path.Join(path.Dir(playlistRel), entry))
	if p == ".." || strings.HasPrefix(p, "../") {
		return ""
	}
	return p
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
		songIDs[p] = id
	}
	srows.Close()

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
		if sid, ok := songAt(resolve(rel, e.Path)); ok {
			songID = sid
		}
		if _, err := tx.ExecContext(ctx, `INSERT INTO playlist_items (playlist_id, pos, song_id, raw_path, title, duration_s)
			VALUES (?, ?, ?, ?, ?, ?)`, id, i, songID, e.Path, e.Title, e.DurationS); err != nil {
			return 0, err
		}
	}
	return id, nil
}

func isPlaylist(name string) bool {
	ext := strings.ToLower(path.Ext(name))
	return ext == ".m3u8" || ext == ".m3u"
}
