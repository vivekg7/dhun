package library

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"os"
	"path"
	"path/filepath"
	"strings"
	"time"
)

// SpecialLists are the playlists every user has and cannot delete
// (docs/plans/010_special_playlists.md). Their truth is the table; the file
// in Playlists/<user>/ is a copy written once a night by ExportSpecial, so
// no user action ever writes to the music share for them.
var SpecialLists = []struct{ Table, Name string }{
	{"favorites", "Favorites"},
	{"listen_later", "Listen Later"},
}

// IsSpecialFile reports whether rel is where a user's special list is
// exported: Playlists/<user>/Favorites.m3u8 and the like. Compared ignoring
// case, as a Mac sees the share.
func IsSpecialFile(rel string) bool {
	parts := strings.Split(rel, "/")
	if len(parts) != 3 || parts[0] != PlaylistsDir {
		return false
	}
	for _, l := range SpecialLists {
		if strings.EqualFold(parts[2], l.Name+".m3u8") {
			return true
		}
	}
	return false
}

// ExportSpecial writes every user's special lists to their Playlists/<user>/
// folder, newest first, so other players see them too. A file is written
// only when its content changed, after the usual copy of the old one, and an
// empty list that never had a file gets none.
func ExportSpecial(ctx context.Context, db *sql.DB, root, data string, now time.Time) (written int, err error) {
	rows, err := db.QueryContext(ctx, `SELECT id, name FROM users ORDER BY id`)
	if err != nil {
		return 0, err
	}
	type user struct {
		id   int64
		name string
	}
	var users []user
	for rows.Next() {
		var u user
		if err := rows.Scan(&u.id, &u.name); err != nil {
			rows.Close()
			return 0, err
		}
		users = append(users, u)
	}
	rows.Close()

	var errs []error
	for _, u := range users {
		for _, l := range SpecialLists {
			ok, err := exportList(ctx, db, root, data, u.id, path.Join(PlaylistsDir, u.name, l.Name+".m3u8"), l.Table, l.Name, now)
			if err != nil {
				errs = append(errs, fmt.Errorf("%s's %s: %w", u.name, l.Name, err))
			}
			if ok {
				written++
			}
		}
	}
	return written, errors.Join(errs...)
}

func exportList(ctx context.Context, db *sql.DB, root, data string, userID int64, rel, table, name string,
	now time.Time) (bool, error) {
	// table is one of SpecialLists' names, never input.
	rows, err := db.QueryContext(ctx, `SELECT s.path, s.title, s.artist, s.duration_ms FROM `+table+` t
		JOIN songs s ON s.id = t.song_id WHERE t.user_id = ? AND NOT t.deleted ORDER BY t.at DESC, t.song_id`, userID)
	if err != nil {
		return false, err
	}
	var b strings.Builder
	fmt.Fprintf(&b, "#EXTM3U\n# Dhun writes this file every night from %s in the app; changes made here are replaced.\n", name)
	n := 0
	for rows.Next() {
		var p, title, artist string
		var durMS int64
		if err := rows.Scan(&p, &title, &artist, &durMS); err != nil {
			rows.Close()
			return false, err
		}
		if artist != "" {
			title = artist + " - " + title
		}
		r, err := filepath.Rel(filepath.FromSlash(path.Dir(rel)), filepath.FromSlash(p))
		if err != nil {
			rows.Close()
			return false, err
		}
		fmt.Fprintf(&b, "#EXTINF:%d,%s\n%s\n", durMS/1000, oneLine(title), filepath.ToSlash(r))
		n++
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return false, err
	}

	abs := filepath.Join(root, filepath.FromSlash(rel))
	old, err := os.ReadFile(abs)
	switch {
	case errors.Is(err, os.ErrNotExist):
		if n == 0 {
			return false, nil
		}
	case err != nil:
		return false, err
	case string(old) == b.String():
		return false, nil
	}
	if err := keepCopy(root, data, rel, "history", now); err != nil {
		return false, err
	}
	return true, writeAtomic(abs, []byte(b.String()))
}
