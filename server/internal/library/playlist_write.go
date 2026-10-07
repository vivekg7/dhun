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
	Before    []string // # lines kept from the file, written before the entry
}

// oneLine drops control characters (newlines above all) from text that goes
// into a single playlist line, so a tag or a name cannot add lines or entries.
func oneLine(s string) string {
	return strings.Map(func(r rune) rune {
		if r < 32 || r == 0x7f {
			return -1
		}
		return r
	}, s)
}

// SavePlaylist rewrites the .m3u8 at rel (creating it if needed) and
// re-indexes it inside tx, returning the playlist's ID. The file as it was
// before the day's first change is kept under data (keepCopy).
//
// The file stays usable in any player: #EXTINF lines and paths relative to
// the playlist's folder, as the existing playlists are written. Header lines
// before the first entry and lines after the last are re-read from the file
// and kept; lines between entries travel with the entry that follows them.
func SavePlaylist(ctx context.Context, tx *sql.Tx, root, data, rel string, existingID int64, owner sql.NullInt64,
	name string, entries []WriteEntry, version int64) (int64, error) {
	abs := filepath.Join(root, filepath.FromSlash(rel))
	if err := keepCopy(root, data, rel, "history", time.Now()); err != nil {
		return 0, err
	}
	header := readHeader(abs)
	var trailer []string
	if old, err := ParsePlaylist(abs); err == nil {
		trailer = old.Trailer
	}

	var b strings.Builder
	if len(header) == 0 || header[0] != "#EXTM3U" {
		b.WriteString("#EXTM3U\n")
	}
	// The name goes where the file had its #PLAYLIST line; a file without
	// one gets it only when the name differs from the file name.
	named := false
	for _, h := range header {
		if strings.HasPrefix(h, "#PLAYLIST:") {
			if named {
				continue
			}
			h, named = "#PLAYLIST:"+oneLine(name), true
		}
		b.WriteString(h + "\n")
	}
	if !named && name != strings.TrimSuffix(path.Base(rel), path.Ext(rel)) {
		fmt.Fprintf(&b, "#PLAYLIST:%s\n", oneLine(name))
	}
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
		for _, l := range e.Before {
			b.WriteString(l + "\n")
		}
		if e.Title != "" || e.DurationS >= 0 { // no #EXTINF is added where there was none
			fmt.Fprintf(&b, "#EXTINF:%d,%s\n", e.DurationS, oneLine(e.Title))
		}
		b.WriteString(line + "\n")
	}
	for _, l := range trailer {
		b.WriteString(l + "\n")
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
	return storePlaylist(ctx, tx, rel, existingID, owner, pl, info, version, songAtTx(ctx, tx))
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

// writeAtomic writes via a temporary file in the same folder and a rename,
// so a reader (another player over SMB, or the scanner) never sees a
// half-written playlist, and a crash or power cut leaves the old file or the
// new one, never a truncated one. The file keeps the mode of the one it
// replaces.
func writeAtomic(abs string, data []byte) error {
	dir := filepath.Dir(abs)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	mode := os.FileMode(0o644)
	if info, err := os.Stat(abs); err == nil {
		mode = info.Mode().Perm()
	}
	tmp, err := os.CreateTemp(dir, ".dhun-*.tmp")
	if err != nil {
		return err
	}
	defer os.Remove(tmp.Name()) // no-op after a successful rename
	_, err = tmp.Write(data)
	if err == nil {
		err = tmp.Chmod(mode)
	}
	if err == nil {
		err = tmp.Sync()
	}
	if cerr := tmp.Close(); err == nil {
		err = cerr
	}
	if err != nil {
		return err
	}
	if err := os.Rename(tmp.Name(), abs); err != nil {
		return err
	}
	if d, err := os.Open(dir); err == nil { // make the rename itself durable
		d.Sync()
		d.Close()
	}
	return nil
}

// keepCopy copies the playlist file at rel, if it exists, to
// data/playlists/<kind>/YYYY-MM-DD/<path below Playlists/>. Every playlist
// file Dhun is about to change, rename or delete goes through here first, so
// nothing a user made is ever lost (AGENTS.md):
//
//   - kind "history" keeps the first copy of the day only, i.e. the file as
//     it was before that day's edits;
//   - kind "deleted" keeps every copy, adding " (2)", " (3)", ….
//
// The copies live in Dhun's data folder rather than Music/_trash, so the
// Music mount can stay read-only apart from Playlists/ (plan 003).
func keepCopy(root, data, rel, kind string, now time.Time) error {
	content, err := os.ReadFile(filepath.Join(root, filepath.FromSlash(rel)))
	if errors.Is(err, os.ErrNotExist) {
		return nil // a new playlist: nothing to keep
	}
	if err != nil {
		return err
	}
	sub := strings.TrimPrefix(rel, PlaylistsDir+"/")
	ext := path.Ext(sub)
	base := path.Join("playlists", kind, now.Format("2006-01-02"), strings.TrimSuffix(sub, ext))
	dst := base + ext
	for i := 2; ; i++ {
		_, err := os.Stat(filepath.Join(data, filepath.FromSlash(dst)))
		if errors.Is(err, os.ErrNotExist) {
			break
		}
		if err != nil {
			return err
		}
		if kind == "history" {
			return nil // today's original is already kept
		}
		dst = fmt.Sprintf("%s (%d)%s", base, i, ext)
	}
	return writeAtomic(filepath.Join(data, filepath.FromSlash(dst)), content)
}

// PlaylistRel returns a free path for a playlist file named name in dir
// (e.g. "Playlists/vivek"), adding " (2)", " (3)", … if needed. except is the
// playlist's current path when renaming, which counts as free.
func PlaylistRel(root, dir, name, except string) (string, error) {
	base := strings.Map(func(r rune) rune {
		if strings.ContainsRune(`/\:*?"<>|`, r) || r < 32 || r == 0x7f {
			return '-'
		}
		return r
	}, strings.TrimSpace(name))
	base = strings.TrimLeft(base, ".")
	if base == "" {
		return "", errors.New("playlist name is empty")
	}
	if r := []rune(base); len(r) > 120 { // by character: never split a UTF-8 sequence
		base = string(r[:120])
	}
	if IsSpecialFile(path.Join(dir, base+".m3u8")) {
		return "", fmt.Errorf("%q is a built-in list", base)
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

// RenamePlaylist renames a playlist file within Playlists/, keeping a copy
// of it first.
func RenamePlaylist(root, data, from, to string, now time.Time) error {
	if from == to {
		return nil
	}
	if err := keepCopy(root, data, from, "history", now); err != nil {
		return err
	}
	// os.Rename replaces an existing file; never let it replace a playlist
	// that appeared under the new name since PlaylistRel looked.
	if _, err := os.Lstat(filepath.Join(root, filepath.FromSlash(to))); !errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("%s already exists", to)
	}
	return os.Rename(filepath.Join(root, filepath.FromSlash(from)), filepath.Join(root, filepath.FromSlash(to)))
}

// TrashPlaylist removes a playlist file from Playlists/ after copying it to
// data/playlists/deleted/YYYY-MM-DD/. A copy and a remove rather than a
// rename: the data folder is a different mount.
func TrashPlaylist(root, data, rel string, now time.Time) error {
	if err := keepCopy(root, data, rel, "deleted", now); err != nil {
		return err
	}
	return os.Remove(filepath.Join(root, filepath.FromSlash(rel)))
}
