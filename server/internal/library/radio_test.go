package library

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"path/filepath"
	"slices"
	"testing"
	"time"

	"github.com/vivekg7/dhun/server/internal/store"
)

type radioFixture struct {
	t  *testing.T
	db *sql.DB
	n  int
}

func newRadioFixture(t *testing.T) *radioFixture {
	db, err := store.Open(filepath.Join(t.TempDir(), "dhun.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { db.Close() })
	for _, u := range []string{"a", "b"} {
		if _, err := db.Exec(`INSERT INTO users (name, password_hash, created_at) VALUES (?, '', '')`, u); err != nil {
			t.Fatal(err)
		}
	}
	return &radioFixture{t: t, db: db}
}

// song adds a music song and returns its ID.
func (f *radioFixture) song(title, artist, lang string, year, score int) int64 {
	f.t.Helper()
	f.n++
	artists, _ := json.Marshal([]string{artist})
	res, err := f.db.Exec(`INSERT INTO songs (path, size, mtime_ns, quick_hash, title, artist, artists, genres,
		language, year, score, duration_ms, added_at, updated_at, version)
		VALUES (?, 0, 0, x'00', ?, ?, ?, '["Bollywood"]', ?, ?, ?, 200000, '', '', 1)`,
		fmt.Sprintf("s%d.mp3", f.n), title, artist, string(artists), lang, year, score)
	if err != nil {
		f.t.Fatal(err)
	}
	id, _ := res.LastInsertId()
	return id
}

func (f *radioFixture) playlist(owner int64, songs ...int64) {
	f.t.Helper()
	var o any
	if owner != 0 {
		o = owner
	}
	res, err := f.db.Exec(`INSERT INTO playlists (path, owner_id, name, size, mtime_ns, version) VALUES (?, ?, 'p', 0, 0, 1)`,
		fmt.Sprintf("Playlists/p%d.m3u8", f.n), o)
	if err != nil {
		f.t.Fatal(err)
	}
	f.n++
	id, _ := res.LastInsertId()
	for i, s := range songs {
		if _, err := f.db.Exec(`INSERT INTO playlist_items (playlist_id, pos, song_id, raw_path) VALUES (?, ?, ?, '')`, id, i, s); err != nil {
			f.t.Fatal(err)
		}
	}
}

func (f *radioFixture) play(user, song int64, at time.Time, ms int64, end string) {
	f.t.Helper()
	if _, err := f.db.Exec(`INSERT INTO plays (user_id, song_id, at, ms_played, end_reason) VALUES (?, ?, ?, ?, ?)`,
		user, song, at.UTC().Format("2006-01-02T15:04:05.000Z"), ms, end); err != nil {
		f.t.Fatal(err)
	}
}

func (f *radioFixture) radio(user, seed int64, n int, exclude ...int64) []int64 {
	f.t.Helper()
	r := &Radio{DB: f.db}
	got, err := r.Similar(context.Background(), user, seed, n, exclude)
	if err != nil {
		f.t.Fatal(err)
	}
	return got
}

// The rules radio must keep whatever the weights say: no other version of
// the seed, no two versions of one song, at most two songs per artist,
// nothing already queued, and an unknown seed is an error, not a guess.
func TestRadioKeepsItsRules(t *testing.T) {
	f := newRadioFixture(t)
	seed := f.song("Tum Hi Ho", "Arijit Singh", "Hindi", 2013, 90)
	reprise := f.song("Tum Hi Ho (Reprise)", "Mithoon", "Hindi", 2013, 80)
	var arijit, others []int64
	for i := range 6 {
		arijit = append(arijit, f.song(fmt.Sprintf("Arijit %d", i), "Arijit Singh", "Hindi", 2013, 90))
	}
	live := f.song("Arijit 0 - Live", "Shreya Ghoshal", "Hindi", 2013, 90)
	for i := range 12 {
		others = append(others, f.song(fmt.Sprintf("Other %d", i), fmt.Sprintf("Singer %d", i), "Hindi", 2012, 85))
	}

	for range 20 {
		got := f.radio(1, seed, 10, others[0])
		if len(got) != 10 {
			t.Fatalf("got %d songs, want 10", len(got))
		}
		n := 0
		for i, id := range got {
			if id == seed || id == reprise || id == others[0] {
				t.Fatalf("radio holds %d: %v", id, got)
			}
			if slices.Contains(arijit, id) {
				n++
				if i < 2 {
					t.Fatalf("the seed's artist in slot %d: %v", i, got)
				}
			}
		}
		if n > 2 {
			t.Fatalf("%d songs by the seed's artist: %v", n, got)
		}
		if slices.Contains(got, arijit[0]) && slices.Contains(got, live) {
			t.Fatalf("two versions of one song: %v", got)
		}
	}

	if _, err := (&Radio{DB: f.db}).Similar(context.Background(), 1, 9999, 10, nil); err != ErrUnknownSong {
		t.Fatalf("unknown seed: err = %v, want ErrUnknownSong", err)
	}
}

// What the family did decides: a song in the seed's curated list, or
// played after it, beats a stranger with the same tags, while another
// user's private list and a skip do not count as "goes with". A song in
// another language loses to one in the seed's.
func TestRadioFollowsListsAndListening(t *testing.T) {
	f := newRadioFixture(t)
	seed := f.song("Seed", "A", "Punjabi", 2010, 50)
	// Each song is placed before the one it must not tie with: equal
	// scores keep the songs' order, so a signal that wrongly counts shows.
	private := f.song("Private", "D", "Punjabi", 2010, 50)
	listed := f.song("Listed", "B", "Punjabi", 2010, 50)
	skipped := f.song("Skipped", "E", "Punjabi", 2010, 50)
	together := f.song("Together", "C", "Punjabi", 2010, 50)
	plain := f.song("Plain", "F", "Punjabi", 2010, 50)
	hindi := f.song("Hindi", "G", "Hindi", 2010, 50)
	f.playlist(0, seed, listed)
	f.playlist(2, seed, private) // user 2's own list
	at := time.Now().Add(-72 * time.Hour)
	for i := range 3 {
		day := at.Add(-time.Duration(i) * 24 * time.Hour)
		f.play(2, seed, day, 200000, "finished")
		f.play(2, together, day.Add(4*time.Minute), 200000, "finished")
		f.play(2, skipped, day.Add(8*time.Minute), 5000, "skipped")
	}

	snap, err := buildRadio(context.Background(), f.db)
	if err != nil {
		t.Fatal(err)
	}
	// No chance here: random() = 1 keeps the scores' order.
	order := snap.similar(snap.index[seed], 1, 10, nil, nil, nil, func() float64 { return 1 })
	if want := []int64{together, listed, private, skipped, plain, hindi}; !slices.Equal(order, want) {
		t.Errorf("order = %v, want %v", order, want)
	}

	// The user's own early skips push a song down; songs heard today go.
	f.play(1, plain, time.Now().Add(-time.Hour), 200000, "finished")
	f.play(1, private, time.Now().Add(-48*time.Hour), 3000, "skipped")
	got := f.radio(1, seed, 10)
	if slices.Contains(got, plain) {
		t.Errorf("a song heard an hour ago came back: %v", got)
	}
	heard, skips, err := (&Radio{DB: f.db}).history(context.Background(), 1)
	if err != nil {
		t.Fatal(err)
	}
	order = snap.similar(snap.index[seed], 1, 10, nil, heard, skips, func() float64 { return 1 })
	if want := []int64{together, listed, skipped, private, hindi}; !slices.Equal(order, want) {
		t.Errorf("after an early skip and a play today, order = %v, want %v", order, want)
	}
}
