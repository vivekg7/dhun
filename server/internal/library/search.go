package library

import (
	"context"
	"database/sql"
	"fmt"
	"os"
	"path"
	"regexp"
	"sort"
	"strings"
	"sync"
	"unicode"

	"golang.org/x/text/unicode/norm"
)

// Search folding, as the Android app's data/Search.kt does it
// (docs/plans/029_search.md); api/search-fold.tsv keeps the two in step.

// FoldWords returns the lower-case words of s: Devanagari in Latin letters,
// accents off, "&" as "and", apostrophes joining and other punctuation
// splitting.
func FoldWords(s string) []string {
	t := spell(strings.ToLower(norm.NFKD.String(devanagari(s))))
	var out []string
	var w strings.Builder
	end := func() {
		if w.Len() > 0 {
			out = append(out, w.String())
		}
		w.Reset()
	}
	for _, c := range t {
		switch {
		case unicode.Is(unicode.M, c):
		case unicode.IsLetter(c) || unicode.IsDigit(c):
			w.WriteRune(c)
		case strings.ContainsRune(apostrophes, c):
		case c == '&':
			end()
			out = append(out, "and")
		default:
			end()
		}
	}
	end()
	return out
}

const apostrophes = "'‘’ʼ`"

// sounds are applied in order; "ch" is set aside while a lone "c" becomes "k".
var sounds = []string{
	"tsch", "ch", "tch", "ch", "sch", "sh", "ph", "f", "ck", "k",
	"ch", "\x01", "c", "k", "\x01", "ch",
	"q", "k", "w", "v", "z", "j",
	"kh", "k", "gh", "g", "th", "t", "dh", "d", "bh", "b", "jh", "j",
	"ue", "u", "oe", "o", "ee", "i", "oo", "u", "ie", "i",
	"ai", "e", "ei", "e", "ay", "e", "ey", "e", "y", "i",
}

// FoldKey is a word's sound key: Kabhi and Kabhie, Main and Mein come out the same.
func FoldKey(word string) string {
	k := word
	for i := 0; i < len(sounds); i += 2 {
		k = strings.ReplaceAll(k, sounds[i], sounds[i+1])
	}
	var b []rune
	for _, c := range k {
		if len(b) == 0 || b[len(b)-1] != c {
			b = append(b, c)
		}
	}
	if n := len(b); n > 2 && b[n-1] == 'h' && strings.ContainsRune("aeiou", b[n-2]) {
		b = b[:n-1]
	}
	return string(b)
}

// spell writes out letters that do not decompose into a letter and an accent.
var spell = strings.NewReplacer("ß", "ss", "æ", "ae", "œ", "oe", "ø", "o", "đ", "d", "ð", "d", "ł", "l", "þ", "th", "ı", "i").Replace

var (
	// U+0915 to U+0939.
	consonants = []string{
		"k", "kh", "g", "gh", "n", "ch", "chh", "j", "jh", "n", "t", "th", "d", "dh", "n", "t", "th", "d", "dh", "n",
		"n", "p", "ph", "b", "bh", "m", "y", "r", "r", "l", "l", "l", "v", "sh", "sh", "s", "h",
	}
	// U+0958 to U+095F, and what a separate nukta does to a consonant.
	nuktaForms = []string{"q", "kh", "g", "z", "d", "dh", "f", "y"}
	nukta      = map[string]string{"k": "q", "j": "z", "ph": "f"}
	// Vowel signs, U+093E to U+094C, and vowels, U+0904 to U+0914.
	signs  = []string{"aa", "i", "ii", "u", "uu", "ri", "ri", "e", "e", "e", "ai", "o", "o", "o", "au"}
	vowels = []string{"a", "a", "aa", "i", "ii", "u", "uu", "ri", "l", "e", "e", "e", "ai", "o", "o", "o", "au"}
)

// letter is a consonant and its vowel: "a" until a sign, a virama ("") or
// the dropping of a final a says otherwise. A vowel or sign on its own is a
// letter with no consonant and closed.
type letter struct {
	latin, vowel string
	open         bool
	consonant    bool
}

// devanagari writes Devanagari in Latin letters, dropping the inherent a
// where Hindi does: at the end of a word, and between a vowel and a
// consonant that has one (धड़कन is dhadkan). Worked from the right, so each
// letter sees what was decided for the next.
func devanagari(s string) string {
	if !strings.ContainsFunc(s, func(r rune) bool { return r >= 0x900 && r <= 0x97f }) {
		return s
	}
	var out strings.Builder
	var word []*letter
	flush := func() {
		for i := len(word) - 1; i >= 0; i-- {
			l := word[i]
			if !l.consonant || !l.open {
				continue
			}
			voweled := i > 0 && (!word[i-1].consonant || word[i-1].vowel != "")
			if i > 0 && (i == len(word)-1 || (word[i+1].consonant && word[i+1].vowel != "" && voweled)) {
				l.vowel = ""
			}
		}
		for _, l := range word {
			out.WriteString(l.latin + l.vowel)
		}
		word = word[:0]
	}
	for _, c := range s {
		var last *letter
		if n := len(word); n > 0 && word[n-1].consonant {
			last = word[n-1]
		}
		switch {
		case c >= 0x915 && c <= 0x939:
			word = append(word, &letter{latin: consonants[c-0x915], vowel: "a", open: true, consonant: true})
		case c >= 0x958 && c <= 0x95f:
			word = append(word, &letter{latin: nuktaForms[c-0x958], vowel: "a", open: true, consonant: true})
		case c == 0x93c:
			if last != nil {
				if v, ok := nukta[last.latin]; ok {
					last.latin = v
				}
			}
		case c >= 0x93e && c <= 0x94c:
			v := signs[c-0x93e]
			if last != nil && last.open {
				last.vowel, last.open = v, false
			} else {
				word = append(word, &letter{latin: v})
			}
		case c == 0x94d:
			if last != nil {
				last.vowel, last.open = "", false
			}
		case c == 0x901 || c == 0x902 || c == 0x903:
			// A nasal or visarga keeps the a before it: हंस is hans.
			if last != nil {
				last.open = false
			}
			sign := "n"
			if c == 0x903 {
				sign = "h"
			}
			word = append(word, &letter{latin: sign})
		case c >= 0x904 && c <= 0x914:
			word = append(word, &letter{latin: vowels[c-0x904]})
		case c == 0x960:
			word = append(word, &letter{latin: "ri"})
		case c == 0x961:
			word = append(word, &letter{latin: "l"})
		case c == 0x93d:
		case c == 0x950:
			flush()
			out.WriteString("om")
		case c >= 0x966 && c <= 0x96f:
			flush()
			out.WriteRune('0' + c - 0x966)
		default:
			flush()
			if c == 0x964 || c == 0x965 {
				c = ' '
			}
			out.WriteRune(c)
		}
	}
	flush()
	return out.String()
}

// LyricsHit is a song whose lyrics hold a search, with the line that does.
type LyricsHit struct {
	Song int64  `json:"song"`
	Line string `json:"line"`
}

// MaxLyricsHits is how many songs a lyrics search returns at most.
const MaxLyricsHits = 50

// LyricsIndex keeps every song's lyrics lines in memory, a few megabytes
// for the family's library, so a search reads no files
// (docs/plans/029_search.md). Each distinct word is kept once; a line is
// the numbers of its words.
type LyricsIndex struct {
	DB   *sql.DB
	Root string

	mu    sync.Mutex
	built bool
	songs map[int64]*lyricsSong
	words []string
	keys  []string
	ids   map[string]int32
}

type lyricsSong struct {
	version int64
	lrc     string // the .lrc file's size and time: an edit to it leaves the song's row alone
	lines   []string
	words   [][]int32
}

var (
	lrcStamp = regexp.MustCompile(`\[\d{1,3}:\d{1,2}(?:[.:]\d{1,3})?\]|<\d{1,3}:\d{1,2}(?:[.:]\d{1,3})?>`)
	lrcTag   = regexp.MustCompile(`^\[[a-zA-Z#]+:.*\]$`)
)

// Refresh brings the index up to date with the library. A song's lyrics
// are read again only when its row or its .lrc file changed, so after the
// first time a refresh is one directory listing per folder.
func (x *LyricsIndex) Refresh(ctx context.Context) error {
	x.mu.Lock()
	defer x.mu.Unlock()
	return x.refresh(ctx)
}

func (x *LyricsIndex) refresh(ctx context.Context) error {
	rows, err := x.DB.QueryContext(ctx, `SELECT id, path, version, embedded_lyrics, lrc FROM songs
		WHERE missing_since IS NULL AND (embedded_lyrics OR lrc)`)
	if err != nil {
		return err
	}
	type row struct {
		id, version int64
		f           SongFile
	}
	var all []row
	for rows.Next() {
		var r row
		if err := rows.Scan(&r.id, &r.f.Path, &r.version, &r.f.EmbeddedLyrics, &r.f.Lrc); err != nil {
			rows.Close()
			return err
		}
		all = append(all, r)
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return err
	}

	if x.ids == nil {
		x.ids = map[string]int32{}
	}
	listed := map[string]map[string]string{} // folder → lower-case .lrc name → size and time
	lrcOf := func(p string) string {
		dir := path.Dir(p)
		names, ok := listed[dir]
		if !ok {
			names = map[string]string{}
			entries, _ := os.ReadDir(abs(x.Root, dir))
			for _, e := range entries {
				if strings.HasSuffix(strings.ToLower(e.Name()), ".lrc") {
					if info, err := e.Info(); err == nil {
						names[strings.ToLower(e.Name())] = fmt.Sprintf("%d/%d", info.Size(), info.ModTime().UnixNano())
					}
				}
			}
			listed[dir] = names
		}
		return names[strings.ToLower(strings.TrimSuffix(path.Base(p), path.Ext(p))+".lrc")]
	}

	songs := make(map[int64]*lyricsSong, len(all))
	for _, r := range all {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		lrc := ""
		if r.f.Lrc {
			lrc = lrcOf(r.f.Path)
		}
		if old := x.songs[r.id]; old != nil && old.version == r.version && old.lrc == lrc {
			songs[r.id] = old
			continue
		}
		l, err := ReadLyrics(x.Root, r.f)
		if err != nil {
			continue // unreadable or gone: found again by the next refresh
		}
		s := &lyricsSong{version: r.version, lrc: lrc}
		for _, line := range strings.Split(l.Text, "\n") {
			line = strings.TrimSpace(line)
			if lrcTag.MatchString(line) {
				continue
			}
			line = strings.TrimSpace(lrcStamp.ReplaceAllString(line, ""))
			words := FoldWords(line)
			if len(words) == 0 {
				continue
			}
			ids := make([]int32, len(words))
			for i, w := range words {
				id, ok := x.ids[w]
				if !ok {
					id = int32(len(x.words))
					x.ids[w] = id
					x.words = append(x.words, w)
					x.keys = append(x.keys, FoldKey(w))
				}
				ids[i] = id
			}
			s.lines = append(s.lines, line)
			s.words = append(s.words, ids)
		}
		songs[r.id] = s
	}
	x.songs = songs
	x.built = true
	return nil
}

// Search finds the songs with a line holding every word of q, by the whole
// word, its start, inside it or the start of its sound key; no typos, among
// thousands of lines they would match too much. Lines with the words as
// typed come first. The first search after a start waits for the index.
func (x *LyricsIndex) Search(ctx context.Context, q string) ([]LyricsHit, error) {
	x.mu.Lock()
	defer x.mu.Unlock()
	if !x.built {
		if err := x.refresh(ctx); err != nil {
			return nil, err
		}
	}
	terms := FoldWords(q)
	if len(terms) == 0 {
		return []LyricsHit{}, nil
	}
	phrase := strings.Join(terms, " ")
	// Which words each term matches, decided once for every distinct word.
	match := make([][]bool, len(terms))
	for i, t := range terms {
		key := FoldKey(t)
		match[i] = make([]bool, len(x.words))
		for id, w := range x.words {
			match[i][id] = strings.Contains(w, t) || (key != "" && strings.HasPrefix(x.keys[id], key))
		}
	}
	ids := make([]int64, 0, len(x.songs))
	for id := range x.songs {
		ids = append(ids, id)
	}
	sort.Slice(ids, func(i, j int) bool { return ids[i] < ids[j] })

	type hit struct {
		LyricsHit
		exact bool
	}
	var hits []hit
	for _, id := range ids {
		s := x.songs[id]
	lines:
		for n, line := range s.words {
			for i := range terms {
				if !anyMarked(line, match[i]) {
					continue lines
				}
			}
			folded := make([]string, len(line))
			for i, w := range line {
				folded[i] = x.words[w]
			}
			hits = append(hits, hit{LyricsHit{id, s.lines[n]}, strings.Contains(strings.Join(folded, " "), phrase)})
			break
		}
	}
	sort.SliceStable(hits, func(i, j int) bool { return hits[i].exact && !hits[j].exact })
	out := make([]LyricsHit, 0, min(len(hits), MaxLyricsHits))
	for _, h := range hits[:min(len(hits), MaxLyricsHits)] {
		out = append(out, h.LyricsHit)
	}
	return out, nil
}

// anyMarked reports whether any word of line is marked in ok.
func anyMarked(line []int32, ok []bool) bool {
	for _, w := range line {
		if ok[w] {
			return true
		}
	}
	return false
}
