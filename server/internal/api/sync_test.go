package api

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/vivekg7/dhun/server/internal/library"
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
	ListenLater []struct {
		Song    int64
		Deleted bool
	}
	Played []struct {
		Song    int64
		Deleted bool
	}
	Resume []struct {
		Song       int64
		Deleted    bool
		PositionMS int64 `json:"positionMs"`
	}
	Settings   map[string]json.RawMessage
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

// Every listen is logged for a future recommender, skips and listens cut
// short included, with the time it happened on the phone, but only one
// heard at least halfway adds to the play count (plan 008).
func TestEveryListenIsLoggedButOnlyHalfOrMoreCounts(t *testing.T) {
	e, phone, _ := syncEnv(t, 2)
	if _, err := e.s.DB.Exec(`UPDATE songs SET duration_ms = 240000`); err != nil {
		t.Fatal(err)
	}
	ist := 330
	e.push(phone, 0,
		// Heard offline yesterday at 10:00 IST, uploaded now.
		o("play", map[string]any{"song": 1, "at": "2026-10-06T04:30:00Z", "endedAt": "2026-10-06T04:34:00Z",
			"ms": 240000, "toMs": 240000, "end": "finished", "utcOffset": ist, "source": "playlist:3", "shuffle": true}),
		o("play", map[string]any{"song": 1, "ms": 120000, "end": "skipped"}),                          // exactly half
		o("play", map[string]any{"song": 2, "ms": 5000, "fromMs": 0, "toMs": 5000, "end": "skipped"}), // a skip
		o("play", map[string]any{"song": 2, "ms": 90000, "end": "interrupted", "endedAt": "2999-01-01T00:00:00Z"}))

	var n int
	e.s.DB.QueryRow(`SELECT count(*) FROM plays`).Scan(&n)
	if n != 4 {
		t.Errorf("logged %d listens, want all 4", n)
	}
	var at, ended, src string
	var off int
	var shuffle bool
	e.s.DB.QueryRow(`SELECT at, ended_at, utc_offset, source, shuffle FROM plays WHERE end_reason = 'finished'`).
		Scan(&at, &ended, &off, &src, &shuffle)
	if at != "2026-10-06T04:30:00.000Z" || ended != "2026-10-06T04:34:00.000Z" || off != ist || src != "playlist:3" || !shuffle {
		t.Errorf("offline listen = %s–%s %+d %q shuffle=%v", at, ended, off, src, shuffle)
	}
	var future int
	e.s.DB.QueryRow(`SELECT count(*) FROM plays WHERE ended_at > ?`, time.Now().UTC().Format(opTime)).Scan(&future)
	if future != 0 {
		t.Error("a listen ended in the future")
	}

	var plays struct{ Plays []struct{ Song, Count int } }
	e.do("GET", "/api/v1/plays", phone, nil, 200, &plays)
	if fmt.Sprint(plays.Plays) != "[{1 2}]" {
		t.Errorf("play counts = %v, want song 1 twice and song 2 not at all", plays.Plays)
	}
}

// An audiobook paused on the phone continues on the laptop at the same
// place, however much else was played in between, and an offline device
// reporting an older place later does not move it back (plan 009).
func TestLongFileResumesOnAnotherDevice(t *testing.T) {
	e, phone, mac := syncEnv(t, 2)
	r := e.push(phone, 0,
		o("setting.set", map[string]any{"name": "longFiles.minMinutes", "value": 20}),
		o("resume.set", map[string]any{"song": 1, "positionMs": 3_600_000}),
		o("setting.set", map[string]any{"name": "../evil", "value": true}))
	if r.Results[2].Status != "rejected" {
		t.Errorf("bad setting name: %+v", r.Results[2])
	}
	got := e.push(mac, 0)
	if len(got.Resume) != 1 || got.Resume[0].PositionMS != 3_600_000 {
		t.Errorf("laptop sees resume points %+v, want song 1 at 1:00:00", got.Resume)
	}
	if string(got.Settings["longFiles.minMinutes"]) != "20" {
		t.Errorf("laptop sees settings %v", got.Settings)
	}

	// The laptop listens on; the phone, offline since an hour ago, reports
	// an older place afterwards.
	e.push(mac, 0, o("resume.set", map[string]any{"song": 1, "positionMs": 5_400_000}))
	e.push(phone, 0, o("resume.set", map[string]any{"song": 1, "positionMs": 3_000_000,
		"at": time.Now().Add(-time.Hour).UTC().Format(time.RFC3339)}))
	if got := e.push(phone, 0); got.Resume[0].PositionMS != 5_400_000 {
		t.Errorf("resume point = %d ms, want the later 5,400,000", got.Resume[0].PositionMS)
	}

	// Finished: forgotten everywhere.
	before := e.push(mac, 0).Version
	e.push(phone, 0, o("resume.unset", map[string]any{"song": 1}))
	if got := e.push(mac, before); len(got.Resume) != 1 || !got.Resume[0].Deleted {
		t.Errorf("after finishing, laptop sees %+v, want song 1 deleted", got.Resume)
	}
}

// Listen Later drops what was heard far enough into, by position, so an
// episode finished over several sittings goes too; both rules are the
// user's settings (plan 010).
func TestListenLaterDropsWhatWasFinished(t *testing.T) {
	e, phone, mac := syncEnv(t, 4)
	if _, err := e.s.DB.Exec(`UPDATE songs SET duration_ms = 600000`); err != nil {
		t.Fatal(err)
	}
	listed := func() string {
		var ids []int64
		for _, l := range e.push(mac, 0).ListenLater {
			ids = append(ids, l.Song)
		}
		slices.Sort(ids)
		return fmt.Sprint(ids)
	}
	e.push(phone, 0,
		o("listen_later.add", map[string]any{"song": 1}),
		o("listen_later.add", map[string]any{"song": 2}),
		o("listen_later.add", map[string]any{"song": 3}),
		o("play", map[string]any{"song": 4, "toMs": 600000}), // not on the list: nothing happens
		// Song 1: the last sitting reached 92%, though it heard only a few minutes.
		o("play", map[string]any{"song": 1, "fromMs": 400000, "toMs": 550000, "ms": 150000}),
		o("play", map[string]any{"song": 2, "toMs": 300000, "ms": 300000})) // half: stays
	if got := listed(); got != "[2 3]" {
		t.Errorf("after listening = %s, want [2 3]", got)
	}

	e.push(phone, 0,
		o("setting.set", map[string]any{"name": "listenLater.finishedPercent", "value": 50}),
		o("play", map[string]any{"song": 2, "toMs": 300000}),
		o("setting.set", map[string]any{"name": "listenLater.autoRemove", "value": false}),
		o("play", map[string]any{"song": 3, "toMs": 600000}),
		o("listen_later.add", map[string]any{"song": 1})) // added again after finishing
	if got := listed(); got != "[1 3]" {
		t.Errorf("after changing the settings = %s, want [1 3]", got)
	}
}

// The special lists reach the music share only through the nightly export:
// newest first, rewritten only when they changed, never indexed as playlists
// of their own, and their names cannot be taken by a normal playlist.
func TestSpecialListsAreExportedNightly(t *testing.T) {
	e, phone, _ := syncEnv(t, 3)
	e.push(phone, 0,
		o("favorite.set", map[string]any{"song": 1, "at": "2026-10-01T10:00:00Z"}),
		o("favorite.set", map[string]any{"song": 2, "at": "2026-10-02T10:00:00Z"}),
		o("listen_later.add", map[string]any{"song": 3}))
	file := filepath.Join(e.s.Root, "Playlists", "vivek", "Favorites.m3u8")
	if _, err := os.Stat(file); !os.IsNotExist(err) {
		t.Fatal("a favorite reached the music share before the nightly export")
	}

	ctx := context.Background()
	if n, err := library.ExportSpecial(ctx, e.s.DB, e.s.Root, e.s.DataDir, time.Now()); err != nil || n != 2 {
		t.Fatalf("export wrote %d files, err %v; want 2", n, err)
	}
	b, _ := os.ReadFile(file)
	got := string(b)
	i1, i2 := strings.Index(got, "../../Library/Artist/Album/01 - Song 1"), strings.Index(got, "../../Library/Artist/Album/02 - Song 2")
	if i1 < 0 || i2 < 0 || i2 > i1 {
		t.Errorf("Favorites.m3u8, want song 2 (newest) then song 1, paths relative to the file:\n%s", got)
	}
	if n, _ := library.ExportSpecial(ctx, e.s.DB, e.s.Root, e.s.DataDir, time.Now()); n != 0 {
		t.Errorf("an unchanged export rewrote %d files", n)
	}

	e.scan()
	var lib libraryResp
	e.do("GET", "/api/v1/library", phone, nil, 200, &lib)
	if len(lib.Playlists) != 0 {
		t.Errorf("the exported copies were indexed as playlists: %+v", lib.Playlists)
	}
	r := e.push(phone, 0, o("playlist.create", map[string]any{"ref": "f", "name": "favorites", "songs": []int64{1}}))
	if r.Results[0].Status != "rejected" {
		t.Error("a normal playlist took the Favorites file name")
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
	e.push(phone, 0, o("queue.create", map[string]any{"queue": "q1", "name": "Q", "songs": []int64{1, 2}, "at": "2026-10-06T08:00:00Z"}))
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

	// A phone with its clock a year ahead must not win every later conflict.
	e.push(phone, 0, o("queue.set_current", map[string]any{"queue": "q1", "song": 1, "positionMs": 1, "at": "2099-01-01T00:00:00Z"}))
	e.push(mac, 0, o("queue.set_current", map[string]any{"queue": "q1", "song": 2, "positionMs": 2}))
	if q := e.queue(phone, "q1"); q.CurrentSong != 2 {
		t.Errorf("a clock in the future won: current = song %d, want song 2", q.CurrentSong)
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
			// The app that made it offline matches it by this ref.
			if p.Ref != "pl-1" {
				t.Errorf("ref = %q, want pl-1", p.Ref)
			}
		}
	}

	// Another family member cannot edit it.
	if r := e.push(priya, 0, o("playlist.remove", map[string]any{"playlist": itoa(id), "song": 1})); r.Results[0].Status != "rejected" {
		t.Error("another user edited a private playlist")
	}

	// The file as it was before today's edits is kept: here, the very first
	// version (one entry), not any later one.
	kept, _ := filepath.Glob(filepath.Join(e.s.DataDir, "playlists/history/*/vivek/Road- Trip.m3u8"))
	if len(kept) != 1 {
		t.Fatalf("kept copies = %v, want one", kept)
	}
	if b, _ := os.ReadFile(kept[0]); strings.Count(string(b), "#EXTINF") != 2 {
		t.Errorf("kept copy is not the day's original:\n%s", b)
	}

	// Remove the second copy of song 1, rename, then delete.
	e.push(phone, 0,
		o("playlist.remove", map[string]any{"playlist": itoa(id), "song": 1, "occurrence": 1}),
		o("playlist.rename", map[string]any{"playlist": itoa(id), "name": "Goa"}))
	if _, err := os.Stat(filepath.Join(e.s.Root, "Playlists/vivek/Goa.m3u8")); err != nil {
		t.Errorf("renamed file: %v", err)
	}
	e.push(phone, 0, o("playlist.delete", map[string]any{"playlist": itoa(id)}))
	if _, err := os.Stat(filepath.Join(e.s.Root, "Playlists/vivek/Goa.m3u8")); !os.IsNotExist(err) {
		t.Errorf("deleted playlist still in Playlists/: %v", err)
	}
	trashed, _ := filepath.Glob(filepath.Join(e.s.DataDir, "playlists/deleted/*/vivek/Goa.m3u8"))
	if len(trashed) != 1 {
		t.Error("a deleted playlist must be kept in the data folder, not deleted")
	}

	// A rescan must not resurrect or duplicate anything Dhun wrote.
	if st, err := e.s.Scanner.Scan(t.Context()); err != nil || st.Playlists != 0 {
		t.Errorf("rescan after Dhun's own writes: %+v %v", st, err)
	}
}

// An edit through Dhun must never undo what someone changed in the file over
// SMB since the last scan, nor drop comments and directives between entries.
func TestPlaylistEditKeepsOutsideChanges(t *testing.T) {
	e, phone, _ := syncEnv(t, 3)
	file := filepath.Join(e.s.Root, "Playlists", "Mix.m3u8")
	write := func(s string) { e.file("Playlists/Mix.m3u8", []byte(s)) }
	write("#EXTM3U\n# made by hand\n../Library/Artist/Album/01 - Song 1.m4a\n")
	e.scan()
	var lib libraryResp
	e.do("GET", "/api/v1/library", phone, nil, 200, &lib)
	id := lib.Playlists[0].ID

	// Edited over SMB, and not rescanned yet.
	write("#EXTM3U\n# made by hand\n../Library/Artist/Album/01 - Song 1.m4a\n" +
		"#EXTGRP:Calm\n# keep me\n../Library/Artist/Album/02 - Song 2.opus\n# the end\n")
	r := e.push(phone, 0, o("playlist.insert", map[string]any{"playlist": itoa(id), "songs": []int64{3}}))
	if r.Results[0].Status != "applied" {
		t.Fatalf("insert: %+v", r.Results[0])
	}
	b, _ := os.ReadFile(file)
	got := string(b)
	for _, want := range []string{"# made by hand", "#EXTGRP:Calm\n# keep me\n../Library/Artist/Album/02 - Song 2.opus",
		"03 - Song 3.mp3\n# the end\n"} {
		if !strings.Contains(got, want) {
			t.Errorf("file lost %q:\n%s", want, got)
		}
	}
	// Untouched entries are byte-for-byte: no #EXTINF added to song 1 or
	// rewritten from tags for song 2 (its file says "Song 2", its tags agree
	// here, so check the shape: exactly one generated line, for song 3).
	if !strings.Contains(got, "# made by hand\n../Library/Artist/Album/01 - Song 1.m4a\n") {
		t.Errorf("song 1's entry was changed:\n%s", got)
	}
	if n := strings.Count(got, "#EXTINF"); n != 1 {
		t.Errorf("%d #EXTINF lines, want only the new entry's:\n%s", n, got)
	}
	if n := strings.Count(got, ".m4a\n") + strings.Count(got, ".opus\n") + strings.Count(got, ".mp3\n"); n != 3 {
		t.Errorf("%d entries, want 3:\n%s", n, got)
	}
}

// A name is one line of the file: a newline in it must not add entries.
func TestPlaylistNameCannotAddLines(t *testing.T) {
	e, phone, _ := syncEnv(t, 1)
	e.push(phone, 0, o("playlist.create", map[string]any{"ref": "x", "name": "Evil\n../../../secret.mp3", "songs": []int64{1}}))
	files, _ := filepath.Glob(filepath.Join(e.s.Root, "Playlists/vivek/*.m3u8"))
	if len(files) != 1 {
		t.Fatalf("files = %v", files)
	}
	b, _ := os.ReadFile(files[0])
	if strings.Count(string(b), "\n") != 4 || strings.Contains(string(b), "\n../../../secret") {
		t.Errorf("name leaked into the file:\n%s", b)
	}
}

// A speed per song is a setting (plan 016): over the years they add up, so a
// setting set back to null makes room for a new one instead of filling the cap.
func TestClearedSettingsDoNotCountTowardTheCap(t *testing.T) {
	e, phone, _ := syncEnv(t, 1)
	for b := 0; b < maxSettings; b += maxOpsPerPush {
		var ops []map[string]any
		for i := b; i < b+maxOpsPerPush; i++ {
			ops = append(ops, o("setting.set", map[string]any{"name": fmt.Sprintf("speed.%d", i), "value": map[string]any{"speed": 1.5}}))
		}
		e.push(phone, 0, ops...)
	}
	extra := o("setting.set", map[string]any{"name": "speed.extra", "value": map[string]any{"speed": 2}})
	if r := e.push(phone, 0, extra); r.Results[0].Status != "rejected" {
		t.Fatalf("setting past the cap: %+v", r.Results[0])
	}
	r := e.push(phone, 0,
		o("setting.set", map[string]any{"name": "speed.0", "value": nil}),
		o("setting.set", map[string]any{"name": "speed.extra", "value": map[string]any{"speed": 2}}))
	if r.Results[0].Status != "applied" || r.Results[1].Status != "applied" {
		t.Errorf("after clearing one: %+v", r.Results)
	}
}
