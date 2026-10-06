package api

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
)

type syncResp struct {
	Version int64
	Queues  []struct {
		ID          string
		Deleted     bool
		Name        string
		Songs       []int64
		CurrentSong int64
		PositionMS  int64 `json:"positionMs"`
	}
	Favorites []struct {
		Song    int64
		Deleted bool
	}
	NowPlaying *nowPlayingJSON
	Results    []opResult
}

var opSeq atomic.Int64

// o builds an operation with a fresh id.
func o(typ string, fields map[string]any) map[string]any {
	m := map[string]any{"id": fmt.Sprintf("op-%d", opSeq.Add(1)), "type": typ}
	for k, v := range fields {
		m[k] = v
	}
	return m
}

func (e *env) push(token string, since int64, ops ...map[string]any) syncResp {
	e.t.Helper()
	var out syncResp
	e.do("POST", "/api/v1/sync", token, map[string]any{"since": since, "ops": ops}, 200, &out)
	return out
}

func (e *env) queue(token, id string) (q struct {
	Name        string
	Songs       []int64
	CurrentSong int64
	PositionMS  int64
}) {
	e.t.Helper()
	for _, x := range e.push(token, 0).Queues {
		if x.ID == id {
			q.Name, q.Songs, q.CurrentSong, q.PositionMS = x.Name, x.Songs, x.CurrentSong, x.PositionMS
			return q
		}
	}
	e.t.Fatalf("queue %s not found", id)
	return q
}

// library with songs 1..n, user vivek (admin) signed in on two devices.
func syncEnv(t *testing.T, n int) (e *env, phone, mac string) {
	e = newEnv(t)
	fx := []string{"a.mp3", "b.m4a", "c.opus"}
	for i := 1; i <= n; i++ {
		// Distinct content per file, so the scanner never treats two as one moved song.
		data := append(fixture(t, fx[i%3]), []byte(fmt.Sprintf("pad-%d", i))...)
		e.file(fmt.Sprintf("Library/Artist/Album/%02d - Song %d.%s", i, i, strings.Split(fx[i%3], ".")[1]), data)
	}
	e.scan()
	e.user("vivek", true)
	return e, e.login("vivek"), e.login("vivek")
}

func ptr(n int64) *int64 { return &n }

// Two devices edit the same queue offline. Positions are relative to a
// neighbouring song, so both edits survive in a sensible order (plan 006).
func TestConcurrentQueueEditsMerge(t *testing.T) {
	e, phone, mac := syncEnv(t, 6)
	e.push(phone, 0, o("queue.create", map[string]any{"queue": "q1", "name": "Drive", "songs": []int64{1, 2, 3, 4}}))

	// Phone (offline) inserts 5 after 2; Mac removes 3 and moves 1 to the end.
	e.push(mac, 0,
		o("queue.remove", map[string]any{"queue": "q1", "songs": []int64{3}}),
		o("queue.move", map[string]any{"queue": "q1", "song": 1, "after": 4}))
	e.push(phone, 0, o("queue.insert", map[string]any{"queue": "q1", "songs": []int64{5}, "after": 2}))

	got := e.queue(phone, "q1").Songs
	if fmt.Sprint(got) != "[2 5 4 1]" {
		t.Errorf("merged queue = %v, want [2 5 4 1]", got)
	}

	// Inserting a song already in the queue moves it: no duplicates (Musicolet).
	e.push(phone, 0, o("queue.insert", map[string]any{"queue": "q1", "songs": []int64{1, 6}, "after": 0}))
	if got := e.queue(phone, "q1").Songs; fmt.Sprint(got) != "[1 6 2 5 4]" {
		t.Errorf("after insert at start = %v, want [1 6 2 5 4]", got)
	}
}

// A flaky connection resends a batch; nothing may be applied twice.
func TestRetriedBatchIsIdempotent(t *testing.T) {
	e, phone, _ := syncEnv(t, 2)
	batch := []map[string]any{
		o("queue.create", map[string]any{"queue": "q1", "name": "Q", "songs": []int64{1}}),
		o("queue.insert", map[string]any{"queue": "q1", "songs": []int64{2}}),
		o("play", map[string]any{"song": 1, "ms": 1000}),
	}
	e.push(phone, 0, batch...)
	res := e.push(phone, 0, batch...)
	for _, r := range res.Results {
		if r.Status != "duplicate" {
			t.Errorf("resent op %s: %s", r.ID, r.Status)
		}
	}
	var plays struct{ Plays []struct{ Count int } }
	e.do("GET", "/api/v1/plays", phone, nil, 200, &plays)
	if len(plays.Plays) != 1 || plays.Plays[0].Count != 1 {
		t.Errorf("plays = %+v, want one play", plays.Plays)
	}
}

func TestTwentyFirstQueueDropsTheLeastRecentlyUsed(t *testing.T) {
	e, phone, _ := syncEnv(t, 1)
	for i := 1; i <= 20; i++ {
		e.push(phone, 0, o("queue.create", map[string]any{"queue": fmt.Sprintf("q%d", i), "name": "Queue", "songs": []int64{1}}))
	}
	// Using q1 makes q2 the least recently used.
	e.push(phone, 0, o("queue.set_current", map[string]any{"queue": "q1", "song": 1, "positionMs": 5}))
	r := e.push(phone, 0, o("queue.create", map[string]any{"queue": "q21", "name": "Queue", "songs": []int64{1}}))

	names := map[string]string{}
	for _, q := range r.Queues {
		names[q.ID] = q.Name
	}
	if len(r.Queues) != 20 {
		t.Errorf("%d queues, want 20", len(r.Queues))
	}
	if _, ok := names["q2"]; ok {
		t.Error("q2 (least recently used) was kept")
	}
	if names["q21"] != "Queue (21)" {
		t.Errorf("new queue named %q, want a unique name", names["q21"])
	}
}

// An older position must never overwrite a newer one; times with and
// without fractions must compare correctly.
func TestLaterChangeWins(t *testing.T) {
	e, phone, mac := syncEnv(t, 2)
	e.push(phone, 0, o("queue.create", map[string]any{"queue": "q1", "name": "Q", "songs": []int64{1, 2}}))
	e.push(mac, 0, o("queue.set_current", map[string]any{"queue": "q1", "song": 2, "positionMs": 9000, "at": "2026-10-06T08:12:03.5Z"}))
	e.push(phone, 0, o("queue.set_current", map[string]any{"queue": "q1", "song": 1, "positionMs": 100, "at": "2026-10-06T08:12:03Z"}))
	if q := e.queue(phone, "q1"); q.CurrentSong != 2 || q.PositionMS != 9000 {
		t.Errorf("current = song %d at %d, want song 2 at 9000", q.CurrentSong, q.PositionMS)
	}

	e.push(phone, 0, o("favorite.set", map[string]any{"song": 1, "at": "2026-10-06T10:00:00Z"}))
	e.push(mac, 0, o("favorite.unset", map[string]any{"song": 1, "at": "2026-10-06T09:00:00Z"})) // older
	r := e.push(phone, 0)
	if len(r.Favorites) != 1 || r.Favorites[0].Deleted {
		t.Errorf("favorites = %+v, want song 1 still a favorite", r.Favorites)
	}
}

func TestDeleteWinsAndPullIsIncremental(t *testing.T) {
	e, phone, mac := syncEnv(t, 2)
	first := e.push(phone, 0, o("queue.create", map[string]any{"queue": "q1", "name": "Q", "songs": []int64{1}}))
	e.push(mac, 0, o("queue.delete", map[string]any{"queue": "q1"}))
	r := e.push(phone, first.Version, o("queue.insert", map[string]any{"queue": "q1", "songs": []int64{2}}))
	if r.Results[0].Status != "rejected" {
		t.Errorf("edit after delete: %s, want rejected (and dropped from the outbox)", r.Results[0].Status)
	}
	if len(r.Queues) != 1 || !r.Queues[0].Deleted {
		t.Errorf("incremental pull = %+v, want the q1 tombstone", r.Queues)
	}
	if full := e.push(phone, 0); len(full.Queues) != 0 {
		t.Errorf("a full pull returned %d queues; tombstones belong in incremental pulls only", len(full.Queues))
	}
}

func TestPlaybackStateForHandoff(t *testing.T) {
	e, phone, mac := syncEnv(t, 1)
	e.push(phone, 0, o("playback.state", map[string]any{"queue": "q1", "song": 1, "positionMs": 133000, "playing": true}))
	var np struct{ NowPlaying *nowPlayingJSON }
	e.do("GET", "/api/v1/now-playing", mac, nil, 200, &np)
	if np.NowPlaying == nil || np.NowPlaying.DeviceName != "Phone" || np.NowPlaying.PositionMS != 133000 {
		t.Errorf("now playing = %+v", np.NowPlaying)
	}
}

func TestPlaylistOpsRewriteTheM3u8(t *testing.T) {
	e, phone, _ := syncEnv(t, 3)
	e.user("priya", false)
	priya := e.login("priya")

	// Created offline and edited in the same batch via its client ref.
	r := e.push(phone, 0,
		o("playlist.create", map[string]any{"ref": "pl-1", "name": "Road: Trip", "songs": []int64{1, 2}}),
		o("playlist.insert", map[string]any{"playlist": "ref:pl-1", "songs": []int64{3}, "after": 1}),
		o("playlist.insert", map[string]any{"playlist": "ref:pl-1", "songs": []int64{1}})) // duplicates allowed
	for _, x := range r.Results {
		if x.Status != "applied" {
			t.Fatalf("op %s: %s %s", x.ID, x.Status, x.Error)
		}
	}
	file := filepath.Join(e.s.Root, "Playlists/vivek/Road- Trip.m3u8")
	data, err := os.ReadFile(file)
	if err != nil {
		t.Fatal(err)
	}
	saans := "#EXTINF:1,A.R. Rahman, Shreya Ghoshal & Mohit Chauhan - Saans\n../../Library/Artist/Album/01 - Song 1.m4a\n"
	want := "#EXTM3U\n#PLAYLIST:Road: Trip\n" + saans +
		"#EXTINF:1,Adele - Hello\n../../Library/Artist/Album/03 - Song 3.mp3\n" +
		"#EXTINF:1,Song 2\n../../Library/Artist/Album/02 - Song 2.opus\n" + saans
	if string(data) != want {
		t.Errorf("file:\n%s\nwant (same style as the existing playlists, paths relative to the playlist):\n%s", data, want)
	}

	var lib libraryResp
	e.do("GET", "/api/v1/library", phone, nil, 200, &lib)
	var id int64
	for _, p := range lib.Playlists {
		if p.Name == "Road: Trip" {
			id = p.ID
			if fmt.Sprint(p.Songs) != "[1 3 2 1]" {
				t.Errorf("indexed songs = %v, want [1 3 2 1]", p.Songs)
			}
		}
	}

	// Another family member cannot edit it.
	if r := e.push(priya, 0, o("playlist.remove", map[string]any{"playlist": itoa(id), "song": 1})); r.Results[0].Status != "rejected" {
		t.Error("another user edited a private playlist")
	}

	// Remove the second copy of song 1, rename, then delete (to _trash).
	e.push(phone, 0,
		o("playlist.remove", map[string]any{"playlist": itoa(id), "song": 1, "occurrence": 1}),
		o("playlist.rename", map[string]any{"playlist": itoa(id), "name": "Goa"}))
	if _, err := os.Stat(filepath.Join(e.s.Root, "Playlists/vivek/Goa.m3u8")); err != nil {
		t.Errorf("renamed file: %v", err)
	}
	e.push(phone, 0, o("playlist.delete", map[string]any{"playlist": itoa(id)}))
	trashed, _ := filepath.Glob(filepath.Join(e.s.Root, "_trash/playlists-*/vivek/Goa.m3u8"))
	if len(trashed) != 1 {
		t.Error("a deleted playlist must be moved to _trash, not deleted")
	}

	// A rescan must not resurrect or duplicate anything Dhun wrote.
	if st, err := e.s.Scanner.Scan(t.Context()); err != nil || st.Playlists != 0 {
		t.Errorf("rescan after Dhun's own writes: %+v %v", st, err)
	}
}
