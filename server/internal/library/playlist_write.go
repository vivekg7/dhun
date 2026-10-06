package library

import (
	"bufio"
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

// WriteEntry is one entry of a playlist being written.
type WriteEntry struct {
	SongPath  string // media-root-relative path of the song; "" if unresolved
	Raw       string // the original line, written back as-is when SongPath is ""
	Title     string // for #EXTINF, conventionally "Artist - Title"
	DurationS int
}

// SavePlaylist rewrites the .m3u8 at rel (creating it if needed) and
// re-indexes it inside tx, returning the playlist's ID.
//
// The file stays usable in any player: #EXTINF lines and paths relative to
// the playlist's folder, as the existing playlists are written. Header lines
// before the first entry are kept; comments between entries are not, since
// they cannot be tied to an entry once the order changes.
func SavePlaylist(ctx context.Context, tx *sql.Tx, root, rel string, existingID int64, owner sql.NullInt64,
	name string, entries []WriteEntry, version int64) (int64, error) {
	abs := filepath.Join(root, filepath.FromSlash(rel))
	header := readHeader(abs)

	var b strings.Builder
	if len(header) == 0 || header[0] != "#EXTM3U" {
		b.WriteString("#EXTM3U\n")
	}
	for _, h := range header {
		if !strings.HasPrefix(h, "#PLAYLIST:") {
			b.WriteString(h + "\n")
		}
	}
	fmt.Fprintf(&b, "#PLAYLIST:%s\n", name)
	dir := path.Dir(rel)
	for _, e := range entries {
		line := e.Raw
		if e.SongPath != "" {
			r, err := filepath.Rel(filepath.FromSlash(dir), filepath.FromSlash(e.SongPath))
			if err != nil {
				return 0, err
			}
			line = filepath.ToSlash(r)
		}
		if line == "" {
			continue
		}
		fmt.Fprintf(&b, "#EXTINF:%d,%s\n%s\n", e.DurationS, e.Title, line)
	}

	if err := writeAtomic(abs, []byte(b.String())); err != nil {
		return 0, err
	}
	info, err := os.Stat(abs)
	if err != nil {
		return 0, err
	}
	pl, err := ParsePlaylist(abs)
	if err != nil {
		return 0, err
	}
	return storePlaylist(ctx, tx, rel, existingID, owner, pl, info, version, func(p string) (int64, bool) {
		var id int64
		err := tx.QueryRowContext(ctx, `SELECT id FROM songs WHERE path = ?`, p).Scan(&id)
		return id, err == nil
	})
}

// readHeader returns the lines before the first entry, minus #EXTINF.
func readHeader(abs string) []string {
	f, err := os.Open(abs)
	if err != nil {
		return nil
	}
	defer f.Close()
	var out []string
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		line := strings.TrimSpace(strings.TrimPrefix(sc.Text(), "\uFEFF"))
		if line == "" {
			continue
		}
		if !strings.HasPrefix(line, "#") || strings.HasPrefix(line, "#EXTINF:") {
			break // the first entry
		}
		out = append(out, line)
	}
	return out
}

// writeAtomic writes via a temporary file and a rename, so a reader (another
// player over SMB, or the scanner) never sees a half-written playlist.
func writeAtomic(abs string, data []byte) error {
	if err := os.MkdirAll(filepath.Dir(abs), 0o755); err != nil {
		return err
	}
	tmp, err := os.CreateTemp(filepath.Dir(abs), ".dhun-*.tmp")
	if err != nil {
		return err
	}
	defer os.Remove(tmp.Name()) // no-op after a successful rename
	if _, err := tmp.Write(data); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	return os.Rename(tmp.Name(), abs)
}

// PlaylistRel returns a free path for a playlist file named name in dir
// (e.g. "Playlists/vivek"), adding " (2)", " (3)", … if needed. except is the
// playlist's current path when renaming, which counts as free.
func PlaylistRel(root, dir, name, except string) (string, error) {
	base := strings.Map(func(r rune) rune {
		if strings.ContainsRune(`/\:*?"<>|`, r) || r < 32 {
			return '-'
		}
		return r
	}, strings.TrimSpace(name))
	base = strings.TrimLeft(base, ".")
	if base == "" {
		return "", errors.New("playlist name is empty")
	}
	if len(base) > 120 {
		base = base[:120]
	}
	for i := 1; i < 1000; i++ {
		candidate := base
		if i > 1 {
			candidate = fmt.Sprintf("%s (%d)", base, i)
		}
		rel := path.Join(dir, candidate+".m3u8")
		if rel == except {
			return rel, nil
		}
		if _, err := os.Stat(filepath.Join(root, filepath.FromSlash(rel))); errors.Is(err, os.ErrNotExist) {
			return rel, nil
		}
	}
	return "", errors.New("no free file name for playlist")
}

// TrashPlaylist moves a playlist file into _trash/playlists-YYYY-MM-DD/,
// keeping its path below Playlists/. Nothing is deleted outright (plan 004).
func TrashPlaylist(root, rel string, now time.Time) error {
	sub := strings.TrimPrefix(rel, PlaylistsDir+"/")
	base := path.Join("_trash", "playlists-"+now.Format("2006-01-02"), strings.TrimSuffix(sub, path.Ext(sub)))
	dst := base + path.Ext(sub)
	for i := 2; ; i++ {
		if _, err := os.Stat(filepath.Join(root, filepath.FromSlash(dst))); errors.Is(err, os.ErrNotExist) {
			break
		}
		dst = fmt.Sprintf("%s (%d)%s", base, i, path.Ext(sub))
	}
	abs := filepath.Join(root, filepath.FromSlash(dst))
	if err := os.MkdirAll(filepath.Dir(abs), 0o755); err != nil {
		return err
	}
	return os.Rename(filepath.Join(root, filepath.FromSlash(rel)), abs)
}
