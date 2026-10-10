package library

import (
	"cmp"
	"context"
	"database/sql"
	"path"
	"regexp"
	"slices"
	"strconv"
	"strings"
)

// The server decides which files make one audiobook and which episodes one
// show, so the three apps cannot drift apart on the edge cases. Rules and
// the shapes on the owner's NAS they were checked against:
// docs/plans/031_podcasts_and_audiobooks.md.

type groupItem struct {
	id                 int64
	path, album, date  string
	disc, track        int
	key, title         string // as stored
	index              int
	newKey, newTitle   string
	newIndex, partSort int
}

// partFolder names a folder that holds one part of a book, such as
// "Mistborn 1 - The Final Empire (1 of 3)" or "CD 2": it rolls up into its
// parent.
var partFolder = regexp.MustCompile(`(?i)(?:^|[\s(\[_-])(?:cd|disc|disk|part|pt\.?)\s*\d+\s*[)\]]?$|\(\s*\d+\s*of\s*\d+\s*\)$`)

// partSuffix is a part marker at the end of an album tag, as in
// "… Ascension  pt 2" or "… Mourning (1 of 2)". The number orders the parts.
var partSuffix = regexp.MustCompile(`(?i)[\s,:_-]*[(\[]?\s*(?:(?:cd|disc|disk|part|pt\.?)\s*(\d+)(?:\s*(?:of|/)\s*\d+)?|(\d+)\s*of\s*\d+)\s*[)\]]?\s*$`)

// bookOf returns the book a file belongs to: the files of one folder (part
// folders included) whose album tags match once part markers are removed.
// Untagged files make one book per folder, named after it.
func bookOf(it *groupItem) (key, title string) {
	dir := path.Dir(it.path)
	for dir != "." && partFolder.MatchString(path.Base(dir)) {
		dir = path.Dir(dir)
	}
	album := strings.TrimSpace(it.album)
	if m := partSuffix.FindStringSubmatchIndex(album); m != nil && m[0] > 0 {
		n := m[2:4] // "pt 2", or else "(2 of 3)"
		if n[0] < 0 {
			n = m[4:6]
		}
		it.partSort, _ = strconv.Atoi(album[n[0]:n[1]])
		album = strings.TrimSpace(album[:m[0]])
	}
	switch {
	case album != "":
		return dir + "\x00" + strings.ToLower(album), album
	case dir != ".":
		return dir + "\x00", path.Base(dir)
	default: // an untagged file loose in Audiobooks/
		return it.path, titleFromFileName(it.path)
	}
}

// regroup recomputes the books or shows of one kind and stores those that
// changed, at version. It returns how many files changed group or place.
func regroup(ctx context.Context, tx *sql.Tx, kind string, version int64) (int, error) {
	rows, err := tx.QueryContext(ctx, `SELECT id, path, album, date, disc, track, group_key, group_title, group_index
		FROM songs WHERE kind = ? AND missing_since IS NULL`, kind)
	if err != nil {
		return 0, err
	}
	var items []*groupItem
	for rows.Next() {
		it := &groupItem{}
		if err := rows.Scan(&it.id, &it.path, &it.album, &it.date, &it.disc, &it.track, &it.key, &it.title, &it.index); err != nil {
			rows.Close()
			return 0, err
		}
		items = append(items, it)
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return 0, err
	}
	group(kind, items)
	n := 0
	for _, it := range items {
		if it.newKey == it.key && it.newTitle == it.title && it.newIndex == it.index {
			continue
		}
		if _, err := tx.ExecContext(ctx, `UPDATE songs SET group_key = ?, group_title = ?, group_index = ?, version = ? WHERE id = ?`,
			it.newKey, it.newTitle, it.newIndex, version, it.id); err != nil {
			return 0, err
		}
		n++
	}
	return n, nil
}

// group sets each item's new group and its place in it.
func group(kind string, items []*groupItem) {
	groups := map[string][]*groupItem{}
	titles := map[string]string{}
	for _, it := range items {
		var key, title string
		if kind == KindAudiobook {
			key, title = bookOf(it)
		} else {
			key = path.Dir(it.path) // a show is a folder of episodes
		}
		if _, ok := titles[key]; !ok || titles[key] == "" {
			titles[key] = title
		}
		groups[key] = append(groups[key], it)
	}
	for key, g := range groups {
		if kind == KindAudiobook {
			slices.SortFunc(g, func(a, b *groupItem) int {
				return cmp.Or(cmp.Compare(a.disc, b.disc), cmp.Compare(a.partSort, b.partSort),
					cmp.Compare(a.track, b.track), naturalCompare(a.path, b.path))
			})
		} else {
			// Oldest first; the apps list them newest first. An episode with no
			// date tag yet sorts before the dated ones.
			slices.SortFunc(g, func(a, b *groupItem) int {
				return cmp.Or(cmp.Compare(a.date, b.date), naturalCompare(a.path, b.path))
			})
			titles[key] = showTitle(key, g)
		}
		gk := artKey(kind + "\x00" + key)
		for i, it := range g {
			it.newKey, it.newTitle, it.newIndex = gk, titles[key], i
		}
	}
}

// showTitle is the album tag most of a show's episodes carry, else its
// folder's name.
func showTitle(dir string, g []*groupItem) string {
	count := map[string]int{}
	best := ""
	for _, it := range g {
		if a := strings.TrimSpace(it.album); a != "" {
			count[a]++
			if count[a] > count[best] || count[a] == count[best] && a < best {
				best = a
			}
		}
	}
	if best == "" && dir != "." {
		best = path.Base(dir)
	}
	return best
}

// naturalCompare orders "P2" before "P10": runs of digits compare as
// numbers, the rest case-insensitively.
func naturalCompare(a, b string) int {
	for a != "" && b != "" {
		da, db := digitRun(a), digitRun(b)
		if da > 0 && db > 0 {
			na, nb := strings.TrimLeft(a[:da], "0"), strings.TrimLeft(b[:db], "0")
			if c := cmp.Or(cmp.Compare(len(na), len(nb)), strings.Compare(na, nb)); c != 0 {
				return c
			}
			a, b = a[da:], b[db:]
			continue
		}
		ca, cb := strings.ToLower(a[:1]), strings.ToLower(b[:1])
		if c := strings.Compare(ca, cb); c != 0 {
			return c
		}
		a, b = a[1:], b[1:]
	}
	return cmp.Compare(len(a), len(b))
}

func digitRun(s string) int {
	i := 0
	for i < len(s) && s[i] >= '0' && s[i] <= '9' {
		i++
	}
	return i
}
