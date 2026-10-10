package api

import (
	"io"
	"os"
	"path/filepath"
	"slices"
	"testing"
)

// spokenEnv adds a podcast root with one episode and its subtitles to a
// library of n songs.
func spokenEnv(t *testing.T, n int) (e *env, phone string, episode int64) {
	e, phone, _ = syncEnv(t, n)
	e.s.Scanner.Podcasts = t.TempDir()
	abs := filepath.Join(e.s.Scanner.Podcasts, "Show", "001 - Ep.opus")
	os.MkdirAll(filepath.Dir(abs), 0o755)
	os.WriteFile(abs, fixture(t, "chapters.opus"), 0o644)
	os.WriteFile(filepath.Join(filepath.Dir(abs), "001 - Ep.en.vtt"), []byte("WEBVTT\n\n00:01.000 --> 00:02.000\nHello there\n"), 0o644)
	e.scan()
	if err := e.s.DB.QueryRow(`SELECT id FROM songs WHERE kind = 'podcast'`).Scan(&episode); err != nil {
		t.Fatal(err)
	}
	return e, phone, episode
}

type libSong struct {
	ID         int64
	Kind       string
	GroupTitle string `json:"groupTitle"`
	Chapters   []struct {
		StartMS int64 `json:"startMs"`
	}
	HasLyrics bool `json:"hasLyrics"`
}

// An app released before podcasts would list every episode as a song, so
// only a client that asks with kinds=all gets them.
func TestOnlyNewAppsReceiveEpisodes(t *testing.T) {
	e, phone, episode := spokenEnv(t, 2)
	var old, all struct{ Songs []libSong }
	e.do("GET", "/api/v1/library?since=0", phone, nil, 200, &old)
	e.do("GET", "/api/v1/library?since=0&kinds=all", phone, nil, 200, &all)
	if len(old.Songs) != 2 || len(all.Songs) != 3 {
		t.Fatalf("songs: %d without kinds, %d with kinds=all; want 2 and 3", len(old.Songs), len(all.Songs))
	}
	ep := all.Songs[slices.IndexFunc(all.Songs, func(s libSong) bool { return s.ID == episode })]
	if ep.Kind != "podcast" || ep.GroupTitle != "Test Show" || len(ep.Chapters) != 2 || !ep.HasLyrics {
		t.Errorf("episode %+v", ep)
	}

	// Streamed from the podcast root, and its subtitles served as lyrics.
	resp := e.do("GET", "/api/v1/stream/"+itoa(episode), phone, nil, 200, nil)
	if b, _ := io.ReadAll(resp.Body); len(b) != len(fixture(t, "chapters.opus")) {
		t.Errorf("streamed %d bytes", len(b))
	}
	var l struct{ Source, Text string }
	e.do("GET", "/api/v1/lyrics/"+itoa(episode), phone, nil, 200, &l)
	if l.Source != "vtt" || l.Text != "[00:01.00]Hello there\n" {
		t.Errorf("lyrics %+v", l)
	}
}

// Played marks: set by a listen reaching 90% into an episode, undone by
// hand, and never set for music, which has play counts instead.
func TestEpisodeIsPlayedAtNinetyPercent(t *testing.T) {
	e, phone, episode := spokenEnv(t, 1)
	e.s.DB.Exec(`UPDATE songs SET duration_ms = 600000`)
	played := func() []int64 {
		var ids []int64
		for _, p := range e.push(phone, 0).Played {
			if !p.Deleted {
				ids = append(ids, p.Song)
			}
		}
		return ids
	}
	e.push(phone, 0,
		o("play", map[string]any{"song": 1, "toMs": 600000}),
		o("play", map[string]any{"song": episode, "toMs": 500000}))
	if got := played(); len(got) != 0 {
		t.Fatalf("played after 83%% and a song: %v", got)
	}
	e.push(phone, 0, o("play", map[string]any{"song": episode, "fromMs": 500000, "toMs": 545000}))
	if got := played(); !slices.Equal(got, []int64{episode}) {
		t.Fatalf("played after 91%% = %v", got)
	}
	e.push(phone, 0, o("played.unset", map[string]any{"song": episode}))
	if got := played(); len(got) != 0 {
		t.Errorf("still played after marking it unplayed: %v", got)
	}
}

// A playlist is an .m3u8 in Music/ with paths relative to it; it cannot
// point at an episode under another root.
func TestPlaylistsHoldMusicOnly(t *testing.T) {
	e, phone, episode := spokenEnv(t, 1)
	res := e.push(phone, 0, o("playlist.create", map[string]any{"ref": "p1", "name": "Mix", "songs": []int64{1, episode}}))
	if res.Results[0].Status != "rejected" {
		t.Errorf("a playlist with an episode was %s", res.Results[0].Status)
	}
}
