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

// The curation tool, outside this repo, scores each song's popularity from
// its plays on JioSaavn, Spotify, YouTube Music and Apple Music, and writes
// the scores by its own song ID, which the index maps to a path
// (docs/plans/041_search_filters_and_popularity.md).
var scoreFiles = [2]string{"_meta/index/stats.jsonl", "_meta/index/songs.jsonl"}

// scoreStamp is the two files' sizes and times: the score is read again
// only when it changes, or when a scan adds songs that need theirs.
type scoreStamp [4]int64

func (s *Scanner) scoreStamp() scoreStamp {
	var st scoreStamp
	for i, name := range scoreFiles {
		if info, err := os.Stat(filepath.Join(s.Root, name)); err == nil {
			st[2*i], st[2*i+1] = info.Size(), info.ModTime().UnixNano()
		}
	}
	return st
}

// readScores returns each path's score, rounded to 1–100. Without the files
// it returns errNoScores, and the scores stay as they were. A line that
// does not parse fails the whole read, since a file caught halfway through
// being written would otherwise drop the scores of every song after it.
func readScores(root string) (map[string]int, error) {
	byID := map[string]int{}
	err := eachLine(filepath.Join(root, scoreFiles[0]), func(line []byte) error {
		var x struct {
			SongID string   `json:"song_id"`
			Score  *float64 `json:"score"`
		}
		if err := json.Unmarshal(line, &x); err != nil {
			return err
		}
		if x.Score != nil {
			byID[x.SongID] = min(100, max(1, int(math.Round(*x.Score))))
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	byPath := make(map[string]int, len(byID))
	err = eachLine(filepath.Join(root, scoreFiles[1]), func(line []byte) error {
		var x struct {
			SongID string `json:"song_id"`
			Path   string `json:"path"`
		}
		if err := json.Unmarshal(line, &x); err != nil {
			return err
		}
		if n, ok := byID[x.SongID]; ok {
			byPath[x.Path] = n
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	return byPath, nil
}

var errNoScores = errors.New("no score files")

func eachLine(name string, fn func([]byte) error) error {
	f, err := os.Open(name)
	if errors.Is(err, fs.ErrNotExist) {
		return errNoScores
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

// applyScores sets each music song's score and gives the rows that changed
// the scan's version, so the apps pull only those. A song the index no
// longer scores goes back to 0.
func applyScores(ctx context.Context, tx *sql.Tx, scores map[string]int, version int64) (int, error) {
	rows, err := tx.QueryContext(ctx, `SELECT id, path, score FROM songs WHERE kind = ?`, KindMusic)
	if err != nil {
		return 0, err
	}
	type change struct {
		id    int64
		score int
	}
	var changes []change
	for rows.Next() {
		var id int64
		var p string
		var had int
		if err := rows.Scan(&id, &p, &had); err != nil {
			rows.Close()
			return 0, err
		}
		if want := scores[p]; want != had {
			changes = append(changes, change{id, want})
		}
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return 0, err
	}
	for _, c := range changes {
		if _, err := tx.ExecContext(ctx, `UPDATE songs SET score = ?, version = ? WHERE id = ?`, c.score, version, c.id); err != nil {
			return 0, err
		}
	}
	return len(changes), nil
}
