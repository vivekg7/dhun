package library

import (
	"bufio"
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"math"
	"os"
	"path/filepath"
)

// The curation tool, outside this repo, writes what it knows of each song
// by its own song ID, which songs.jsonl maps to a path: the popularity
// score from the plays on JioSaavn, Spotify, YouTube Music and Apple Music
// in stats.jsonl, and the language in attributes.jsonl
// (docs/plans/041_search_filters_and_popularity.md).
var indexFiles = [3]string{"_meta/index/songs.jsonl", "_meta/index/stats.jsonl", "_meta/index/attributes.jsonl"}

// indexStamp is the files' sizes and times: the index is read again only
// when one changes, or when a scan adds songs that need theirs.
type indexStamp [2 * len(indexFiles)]int64

func (s *Scanner) indexStamp() indexStamp {
	var st indexStamp
	for i, name := range indexFiles {
		if info, err := os.Stat(filepath.Join(s.Root, name)); err == nil {
			st[2*i], st[2*i+1] = info.Size(), info.ModTime().UnixNano()
		}
	}
	return st
}

// songIndex is what the index says of the songs, by path. A nil map is a
// file that is not there: that field stays as it was.
type songIndex struct {
	scores    map[string]int
	languages map[string]string
}

// readIndex reads the index files. Without songs.jsonl, or without both of
// the others, it returns errNoIndex. A line that does not parse fails the
// whole read, since a file caught halfway through being written would
// otherwise drop the values of every song after it.
func readIndex(root string) (songIndex, error) {
	var idx songIndex
	scores := map[string]int{}
	err := eachLine(filepath.Join(root, indexFiles[1]), func(line []byte) error {
		var x struct {
			SongID string   `json:"song_id"`
			Score  *float64 `json:"score"`
		}
		if err := json.Unmarshal(line, &x); err != nil {
			return err
		}
		if x.Score != nil {
			scores[x.SongID] = min(100, max(1, int(math.Round(*x.Score))))
		}
		return nil
	})
	if err != nil && !errors.Is(err, errNoIndex) {
		return idx, err
	}
	haveScores := err == nil

	// An instrumental track is "Instrumental" whatever language it is filed
	// under: JioSaavn files piano albums as English and film themes as
	// Hindi, and the owner wants neither under that language (2026-11-14).
	languages := map[string]string{}
	err = eachLine(filepath.Join(root, indexFiles[2]), func(line []byte) error {
		var x struct {
			SongID       string `json:"song_id"`
			Language     string `json:"language_name"`
			Instrumental bool   `json:"instrumental"`
		}
		if err := json.Unmarshal(line, &x); err != nil {
			return err
		}
		if x.Instrumental {
			languages[x.SongID] = "Instrumental"
		} else if x.Language != "" {
			languages[x.SongID] = x.Language
		}
		return nil
	})
	if err != nil && !errors.Is(err, errNoIndex) {
		return idx, err
	}
	haveLanguages := err == nil
	if !haveScores && !haveLanguages {
		return idx, errNoIndex
	}

	if haveScores {
		idx.scores = make(map[string]int, len(scores))
	}
	if haveLanguages {
		idx.languages = make(map[string]string, len(languages))
	}
	err = eachLine(filepath.Join(root, indexFiles[0]), func(line []byte) error {
		var x struct {
			SongID string `json:"song_id"`
			Path   string `json:"path"`
		}
		if err := json.Unmarshal(line, &x); err != nil {
			return err
		}
		if n, ok := scores[x.SongID]; ok && haveScores {
			idx.scores[x.Path] = n
		}
		if l, ok := languages[x.SongID]; ok && haveLanguages {
			idx.languages[x.Path] = l
		}
		return nil
	})
	return idx, err
}

var errNoIndex = errors.New("no index files")

func eachLine(name string, fn func([]byte) error) error {
	f, err := os.Open(name)
	if errors.Is(err, fs.ErrNotExist) {
		return errNoIndex
	}
	if err != nil {
		return err
	}
	defer f.Close()
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 64<<10), 4<<20)
	for n := 1; sc.Scan(); n++ {
		if len(sc.Bytes()) == 0 {
			continue
		}
		if err := fn(sc.Bytes()); err != nil {
			return fmt.Errorf("%s line %d: %w", filepath.Base(name), n, err)
		}
	}
	return sc.Err()
}

// applyIndex sets each music song's score and language and gives the rows
// that changed the scan's version, so the apps pull only those. A song the
// index no longer has goes back to none.
func applyIndex(ctx context.Context, tx *sql.Tx, idx songIndex, version int64) (int, error) {
	rows, err := tx.QueryContext(ctx, `SELECT id, path, score, language FROM songs WHERE kind = ?`, KindMusic)
	if err != nil {
		return 0, err
	}
	type change struct {
		id       int64
		score    int
		language string
	}
	var changes []change
	for rows.Next() {
		var c change
		var p string
		if err := rows.Scan(&c.id, &p, &c.score, &c.language); err != nil {
			rows.Close()
			return 0, err
		}
		changed := false
		if idx.scores != nil && idx.scores[p] != c.score {
			c.score, changed = idx.scores[p], true
		}
		if idx.languages != nil && idx.languages[p] != c.language {
			c.language, changed = idx.languages[p], true
		}
		if changed {
			changes = append(changes, c)
		}
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return 0, err
	}
	for _, c := range changes {
		if _, err := tx.ExecContext(ctx, `UPDATE songs SET score = ?, language = ?, version = ? WHERE id = ?`,
			c.score, c.language, version, c.id); err != nil {
			return 0, err
		}
	}
	return len(changes), nil
}
