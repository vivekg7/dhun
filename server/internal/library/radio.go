package library

import (
	"cmp"
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"math"
	"math/rand/v2"
	"regexp"
	"slices"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/vivekg7/dhun/server/internal/store"
)

// Radio picks the songs that go with one song (docs/plans/042_play_radio.md).
// What it compares is kept in memory, built again by Refresh after each
// scan, so a request reads only the caller's own recent plays.
type Radio struct {
	DB *sql.DB

	mu   sync.Mutex
	snap *radioSnap
}

// The weights of the signals, in one place to be tuned by listening. Each
// signal is between 0 and 1 (the playlist one up to maxPlaylist), so a
// weight is how many points a perfect match on it is worth.
const (
	wPlaylist  = 2.0  // shared playlists, a long list counting less
	wTogether  = 3.0  // played within half an hour of each other
	wArtist    = 1.5  // any artist in common
	wComposer  = 1.5  // the same music director
	wGenre     = 1.0  // genres in common, of all the two have
	wLanguage  = 2.0  // the same language; a different one costs as much
	wYear      = 1.5  // close years, nothing past ten apart
	wScoreDist = 1.0  // similar popularity
	wScore     = 0.5  // popularity itself
	wSkipped   = -3.0 // this user skips it early

	maxPlaylist   = 2.0 // a song in many of the seed's lists still counts twice at most
	playlistSmall = 30  // a list this long or shorter counts fully; a longer one by 30/size
	togetherFull  = 3   // this many listens together count fully
	togetherGap   = 30 * time.Minute
	neighbours    = 50 // "played together" songs kept per song
	earlySkip     = 30_000

	radioPool     = 40 // the best this many are drawn from at random
	perArtist     = 2
	seedArtistIn  = 2 // no song by the seed's main artist in the first this many
	recentlyHeard = 24 * time.Hour
)

type radioSnap struct {
	songs []radioSong
	index map[int64]int32 // song ID → its place in songs
	lists []radioList
}

type radioSong struct {
	id        int64
	title     string // the folded title without "(Reprise)" and the like
	artists   []int32
	composers []int32
	genres    []int32
	language  string
	year      int
	score     int
	lists     []int32
	together  []radioPair
}

type radioList struct {
	owner int64 // 0: shared
	songs []int32
}

type radioPair struct {
	song  int32
	count int32
}

// Refresh builds what radio compares from the library, the playlists and
// every user's plays.
func (r *Radio) Refresh(ctx context.Context) error {
	snap, err := buildRadio(ctx, r.DB)
	if err != nil {
		return err
	}
	r.mu.Lock()
	r.snap = snap
	r.mu.Unlock()
	return nil
}

func (r *Radio) current(ctx context.Context) (*radioSnap, error) {
	r.mu.Lock()
	snap := r.snap
	r.mu.Unlock()
	if snap != nil {
		return snap, nil
	}
	// The first request after a start, before the first scan finished.
	if err := r.Refresh(ctx); err != nil {
		return nil, err
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	return r.snap, nil
}

// versionMarks are what makes another recording of one song: "Tum Hi Ho
// (Reprise)", "Kesariya [Lofi]", "Kun Faya Kun - Live".
var versionMarks = regexp.MustCompile(`\s*[(\[].*?[)\]]|\s+-\s.*$`)

// BaseTitle is a title folded as search folds it, without what marks one
// version of a song from another.
func BaseTitle(title string) string {
	base := versionMarks.ReplaceAllString(title, "")
	if strings.TrimSpace(base) == "" {
		base = title
	}
	return strings.Join(FoldWords(base), " ")
}

func buildRadio(ctx context.Context, db *sql.DB) (*radioSnap, error) {
	type row struct {
		id                                     int64
		title, artists, composer, genres, lang string
		year, score                            int
	}
	rows, err := store.Rows(ctx, db, `SELECT id, title, artists, composer, genres, language, year, score
		FROM songs WHERE kind = ? AND missing_since IS NULL ORDER BY id`, []any{KindMusic},
		func(rs *sql.Rows) (r row, err error) {
			err = rs.Scan(&r.id, &r.title, &r.artists, &r.composer, &r.genres, &r.lang, &r.year, &r.score)
			return r, err
		})
	if err != nil {
		return nil, err
	}
	names := map[string]int32{}
	intern := func(values []string) []int32 {
		out := make([]int32, 0, len(values))
		for _, v := range values {
			k := strings.ToLower(strings.TrimSpace(v))
			if k == "" {
				continue
			}
			id, ok := names[k]
			if !ok {
				id = int32(len(names))
				names[k] = id
			}
			if !slices.Contains(out, id) {
				out = append(out, id)
			}
		}
		return out
	}
	snap := &radioSnap{songs: make([]radioSong, len(rows)), index: make(map[int64]int32, len(rows))}
	for i, r := range rows {
		var artists, genres []string
		json.Unmarshal([]byte(r.artists), &artists)
		json.Unmarshal([]byte(r.genres), &genres)
		snap.songs[i] = radioSong{
			id:        r.id,
			title:     BaseTitle(r.title),
			artists:   intern(artists),
			composers: intern(splitOn(nameSeparators, []string{r.composer})),
			genres:    intern(genres),
			language:  r.lang,
			year:      r.year,
			score:     r.score,
		}
		snap.index[r.id] = int32(i)
	}

	// Playlists: the shared ones and each user's own, by who may see them.
	type item struct{ list, song int64 }
	type listRow struct{ id, owner int64 }
	lists, err := store.Rows(ctx, db, `SELECT id, COALESCE(owner_id, 0) FROM playlists WHERE NOT deleted ORDER BY id`, nil,
		func(rs *sql.Rows) (l listRow, err error) {
			err = rs.Scan(&l.id, &l.owner)
			return l, err
		})
	if err != nil {
		return nil, err
	}
	listIndex := map[int64]int32{}
	for _, l := range lists {
		listIndex[l.id] = int32(len(snap.lists))
		snap.lists = append(snap.lists, radioList{owner: l.owner})
	}
	items, err := store.Rows(ctx, db, `SELECT playlist_id, song_id FROM playlist_items WHERE song_id IS NOT NULL`, nil,
		func(rs *sql.Rows) (it item, err error) {
			err = rs.Scan(&it.list, &it.song)
			return it, err
		})
	if err != nil {
		return nil, err
	}
	for _, it := range items {
		li, ok := listIndex[it.list]
		si, ok2 := snap.index[it.song]
		if !ok || !ok2 || slices.Contains(snap.songs[si].lists, li) {
			continue
		}
		snap.lists[li].songs = append(snap.lists[li].songs, si)
		snap.songs[si].lists = append(snap.songs[si].lists, li)
	}

	// Played together: two songs one person heard, each through the half
	// that counts a play, within half an hour. A skip is no "goes with".
	type play struct {
		user, song int64
		at         string
	}
	plays, err := store.Rows(ctx, db, `SELECT p.user_id, p.song_id, p.at FROM plays p JOIN songs s ON s.id = p.song_id
		WHERE s.kind = ? AND p.ms_played > 0 AND p.ms_played * 2 >= s.duration_ms
		ORDER BY p.user_id, p.at`, []any{KindMusic},
		func(rs *sql.Rows) (p play, err error) {
			err = rs.Scan(&p.user, &p.song, &p.at)
			return p, err
		})
	if err != nil {
		return nil, err
	}
	together := map[[2]int32]int32{}
	times := make([]time.Time, len(plays))
	for i, p := range plays {
		times[i], _ = time.Parse(time.RFC3339Nano, p.at)
	}
	for i, p := range plays {
		a, ok := snap.index[p.song]
		if !ok {
			continue
		}
		for j := i + 1; j < len(plays) && plays[j].user == p.user && times[j].Sub(times[i]) <= togetherGap; j++ {
			b, ok := snap.index[plays[j].song]
			if !ok || a == b {
				continue
			}
			together[[2]int32{min(a, b), max(a, b)}]++
		}
	}
	for k, n := range together {
		snap.songs[k[0]].together = append(snap.songs[k[0]].together, radioPair{k[1], n})
		snap.songs[k[1]].together = append(snap.songs[k[1]].together, radioPair{k[0], n})
	}
	for i := range snap.songs {
		t := snap.songs[i].together
		slices.SortFunc(t, func(x, y radioPair) int { return cmp.Or(cmp.Compare(y.count, x.count), cmp.Compare(x.song, y.song)) })
		snap.songs[i].together = t[:min(len(t), neighbours)]
	}
	return snap, ctx.Err()
}

// ErrUnknownSong is a seed that is not a song in the music library.
var ErrUnknownSong = errors.New("not a song in the music library")

// Similar returns up to n songs to play after seed for user, best first,
// leaving out exclude (what the queue already holds).
func (r *Radio) Similar(ctx context.Context, user, seed int64, n int, exclude []int64) ([]int64, error) {
	snap, err := r.current(ctx)
	if err != nil {
		return nil, err
	}
	si, ok := snap.index[seed]
	if !ok {
		return nil, ErrUnknownSong
	}
	heard, skips, err := r.history(ctx, user)
	if err != nil {
		return nil, err
	}
	return snap.similar(si, user, n, exclude, heard, skips, rand.Float64), nil
}

// skipRate is how often a user skipped a song early, against how often
// they heard it.
type skipRate struct{ early, heard int }

// history is the user's songs heard in the last day, which radio leaves
// out, and how often they skip each song early.
func (r *Radio) history(ctx context.Context, user int64) (map[int64]bool, map[int64]skipRate, error) {
	since := time.Now().Add(-recentlyHeard).UTC().Format("2006-01-02T15:04:05.000Z")
	type row struct {
		song   int64
		recent bool
		early  int
		heard  int
	}
	rows, err := store.Rows(ctx, r.DB, `SELECT song_id, max(at >= ?),
		sum(end_reason = 'skipped' AND ms_played < ?), sum(end_reason <> 'skipped' OR ms_played >= ?)
		FROM plays WHERE user_id = ? AND ms_played > 0 GROUP BY song_id`,
		[]any{since, earlySkip, earlySkip, user}, func(rs *sql.Rows) (x row, err error) {
			err = rs.Scan(&x.song, &x.recent, &x.early, &x.heard)
			return x, err
		})
	if err != nil {
		return nil, nil, err
	}
	heard, skips := map[int64]bool{}, map[int64]skipRate{}
	for _, x := range rows {
		if x.recent {
			heard[x.song] = true
		}
		if x.early > 0 {
			skips[x.song] = skipRate{x.early, x.heard}
		}
	}
	return heard, skips, nil
}

func (snap *radioSnap) similar(si int32, user int64, n int, exclude []int64, heard map[int64]bool,
	skips map[int64]skipRate, random func() float64) []int64 {
	seed := &snap.songs[si]
	listed := make([]float64, len(snap.songs))
	for _, li := range seed.lists {
		l := snap.lists[li]
		if l.owner != 0 && l.owner != user {
			continue
		}
		w := min(1, float64(playlistSmall)/float64(len(l.songs)))
		for _, s := range l.songs {
			listed[s] += w
		}
	}
	together := map[int32]int32{}
	for _, p := range seed.together {
		together[p.song] = p.count
	}
	skip := map[int64]bool{seed.id: true}
	for _, id := range exclude {
		skip[id] = true
	}

	type cand struct {
		i     int32
		score float64
	}
	cands := make([]cand, 0, len(snap.songs))
	for i := range snap.songs {
		c := &snap.songs[i]
		if skip[c.id] || heard[c.id] || (seed.title != "" && c.title == seed.title) {
			continue
		}
		s := wPlaylist*min(maxPlaylist, listed[i]) +
			wTogether*min(1, float64(together[int32(i)])/togetherFull)
		if shares(seed.artists, c.artists) {
			s += wArtist
		}
		if shares(seed.composers, c.composers) {
			s += wComposer
		}
		if g := len(seed.genres) + len(c.genres); g > 0 {
			common := 0
			for _, x := range c.genres {
				if slices.Contains(seed.genres, x) {
					common++
				}
			}
			s += wGenre * float64(common) / float64(g-common)
		}
		if seed.language != "" && c.language != "" {
			if seed.language == c.language {
				s += wLanguage
			} else {
				s -= wLanguage
			}
		}
		if seed.year > 0 && c.year > 0 {
			s += wYear * max(0, 1-math.Abs(float64(seed.year-c.year))/10)
		}
		if seed.score > 0 && c.score > 0 {
			s += wScoreDist * max(0, 1-math.Abs(float64(seed.score-c.score))/30)
		}
		s += wScore * float64(c.score) / 100
		if k, ok := skips[c.id]; ok {
			s += wSkipped * float64(k.early) / float64(k.early+k.heard+1)
		}
		cands = append(cands, cand{int32(i), s})
	}
	slices.SortFunc(cands, func(a, b cand) int { return cmp.Or(cmp.Compare(b.score, a.score), cmp.Compare(a.i, b.i)) })

	// A little chance: the best radioPool in an order drawn by score
	// (Efraimidis and Spirakis' weighted sampling), then the rest by score.
	// Only songs scoring half the best's or more are drawn: in a small
	// library the best 40 reach songs in another language, which chance
	// would otherwise put up among the good ones.
	pool := cands[:min(len(cands), radioPool)]
	if len(pool) > 0 && pool[0].score > 0 {
		pool = pool[:sort.Search(len(pool), func(i int) bool { return pool[i].score < pool[0].score/2 })]
	}
	low := 0.0
	if len(pool) > 0 {
		low = pool[len(pool)-1].score
	}
	keys := make(map[int32]float64, len(pool))
	for _, c := range pool {
		keys[c.i] = math.Pow(random(), 1/(c.score-low+0.5))
	}
	// pool shares cands' array, so this reorders the head of cands.
	slices.SortStableFunc(pool, func(a, b cand) int { return cmp.Compare(keys[b.i], keys[a.i]) })

	var seedArtist int32 = -1
	if len(seed.artists) > 0 {
		seedArtist = seed.artists[0]
	}
	perArtistCount := map[int32]int{}
	titles := map[string]bool{}
	out := make([]int64, 0, n)
	var held []int32
	take := func(i int32) bool {
		c := &snap.songs[i]
		if titles[c.title] && c.title != "" {
			return false
		}
		for _, a := range c.artists {
			if perArtistCount[a] >= perArtist {
				return false
			}
		}
		for _, a := range c.artists {
			perArtistCount[a]++
		}
		titles[c.title] = true
		out = append(out, c.id)
		return true
	}
	for _, c := range cands {
		if len(out) >= n {
			break
		}
		if len(out) < seedArtistIn && slices.Contains(snap.songs[c.i].artists, seedArtist) {
			held = append(held, c.i)
			continue
		}
		take(c.i)
		if len(out) == seedArtistIn {
			for _, h := range held {
				if len(out) < n {
					take(h)
				}
			}
			held = nil
		}
	}
	// Too few others to fill the first slots: the seed's artist after all.
	for _, h := range held {
		if len(out) < n {
			take(h)
		}
	}
	return out
}

func shares(a, b []int32) bool {
	for _, x := range a {
		if slices.Contains(b, x) {
			return true
		}
	}
	return false
}
