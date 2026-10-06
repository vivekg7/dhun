package api

import (
	"bytes"
	"context"
	"encoding/json"
	"image"
	"image/color"
	"image/png"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"testing"

	"github.com/vivekg7/dhun/server/internal/library"
	"github.com/vivekg7/dhun/server/internal/store"
)

type env struct {
	t   *testing.T
	srv *httptest.Server
	s   *Server
}

func newEnv(t *testing.T) *env {
	t.Helper()
	root := t.TempDir()
	db, err := store.Open(filepath.Join(root, "_dhun", "dhun.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { db.Close() })
	log := slog.New(slog.NewTextHandler(io.Discard, nil))
	sc := &library.Scanner{DB: db, Root: root, Log: log}
	s := &Server{DB: db, Root: root, DataDir: filepath.Join(root, "_dhun"), Scanner: sc, Log: log}
	s.Rescan = func() { sc.Scan(context.Background()) }
	srv := httptest.NewServer(s.Handler())
	t.Cleanup(srv.Close)
	return &env{t: t, srv: srv, s: s}
}

func (e *env) file(rel string, data []byte) {
	e.t.Helper()
	abs := filepath.Join(e.s.Root, filepath.FromSlash(rel))
	os.MkdirAll(filepath.Dir(abs), 0o755)
	if err := os.WriteFile(abs, data, 0o644); err != nil {
		e.t.Fatal(err)
	}
}

func fixture(t *testing.T, name string) []byte {
	data, err := os.ReadFile(filepath.Join("..", "library", "testdata", name))
	if err != nil {
		t.Fatal(err)
	}
	return data
}

func (e *env) scan() {
	e.t.Helper()
	if _, err := e.s.Scanner.Scan(context.Background()); err != nil {
		e.t.Fatal(err)
	}
}

func (e *env) user(name string, admin bool) {
	e.t.Helper()
	if err := CreateUser(context.Background(), e.s.DB, name, "correct horse", admin); err != nil {
		e.t.Fatal(err)
	}
}

func (e *env) login(name string) string {
	e.t.Helper()
	var out struct{ Token string }
	e.do("POST", "/api/v1/login", "", map[string]string{"username": name, "password": "correct horse", "device": "Phone"}, 200, &out)
	return out.Token
}

// do sends a request and checks the status; out, if non-nil, receives the JSON body.
func (e *env) do(method, path, token string, body any, want int, out any) *http.Response {
	e.t.Helper()
	var rd io.Reader
	if body != nil {
		b, _ := json.Marshal(body)
		rd = bytes.NewReader(b)
	}
	req, _ := http.NewRequest(method, e.srv.URL+path, rd)
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		e.t.Fatal(err)
	}
	defer resp.Body.Close()
	data, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != want {
		e.t.Fatalf("%s %s: status %d, want %d: %s", method, path, resp.StatusCode, want, data)
	}
	if out != nil {
		if err := json.Unmarshal(data, out); err != nil {
			e.t.Fatalf("%s %s: %v: %s", method, path, err, data)
		}
	}
	resp.Body = io.NopCloser(bytes.NewReader(data))
	return resp
}

type libraryResp struct {
	Version   int64
	Songs     []songJSON
	Playlists []playlistJSON
}

func TestAuthFlowAndDeviceRevocation(t *testing.T) {
	e := newEnv(t)
	e.user("vivek", true)

	e.do("GET", "/api/v1/library", "", nil, 401, nil)
	e.do("POST", "/api/v1/login", "", map[string]string{"username": "vivek", "password": "wrong", "device": "x"}, 401, nil)

	phone := e.login("vivek")
	mac := e.login("Vivek") // user names are case-insensitive
	var me struct{ User userJSON }
	e.do("GET", "/api/v1/me", mac, nil, 200, &me)
	if me.User.Name != "vivek" || !me.User.Admin {
		t.Errorf("me = %+v", me.User)
	}

	var devs struct{ Devices []struct{ ID int64 } }
	e.do("GET", "/api/v1/devices", mac, nil, 200, &devs)
	if len(devs.Devices) != 2 {
		t.Fatalf("%d devices, want 2", len(devs.Devices))
	}
	// Sign out the lost phone from the Mac.
	var phoneMe struct{ DeviceID int64 }
	e.do("GET", "/api/v1/me", phone, nil, 200, &phoneMe)
	e.do("DELETE", "/api/v1/devices/"+itoa(phoneMe.DeviceID), mac, nil, 204, nil)
	e.do("GET", "/api/v1/me", phone, nil, 401, nil)
	e.do("GET", "/api/v1/me", mac, nil, 200, nil)
}

func TestLibraryFeedIsIncrementalAndPlaylistsArePrivate(t *testing.T) {
	e := newEnv(t)
	e.user("vivek", true)
	e.user("priya", false)
	e.file("Library/Adele/25/1-01 - Hello.mp3", fixture(t, "a.mp3"))
	e.file("Playlists/Shared.m3u8", []byte("../Library/Adele/25/1-01 - Hello.mp3\n"))
	e.file("Playlists/vivek/Mine.m3u8", []byte("../../Library/Adele/25/1-01 - Hello.mp3\n../../Library/Gone.mp3\n"))
	e.file("Playlists/priya/Hers.m3u8", []byte("../../Library/Adele/25/1-01 - Hello.mp3\n"))
	e.scan()
	tok := e.login("vivek")

	var full libraryResp
	e.do("GET", "/api/v1/library?since=0", tok, nil, 200, &full)
	if len(full.Songs) != 1 || full.Songs[0].Title != "Hello" {
		t.Fatalf("songs = %+v", full.Songs)
	}
	names := map[string][]int64{}
	for _, p := range full.Playlists {
		names[p.Name] = p.Songs
	}
	if _, ok := names["Hers"]; ok {
		t.Error("another user's playlist is visible")
	}
	if got := names["Mine"]; len(got) != 2 || got[1] != 0 {
		t.Errorf("Mine = %v, want [id 0] with the unresolved entry kept as 0", got)
	}

	var delta libraryResp
	e.do("GET", "/api/v1/library?since="+itoa(full.Version), tok, nil, 200, &delta)
	if len(delta.Songs)+len(delta.Playlists) != 0 {
		t.Errorf("unchanged library returned %d songs, %d playlists", len(delta.Songs), len(delta.Playlists))
	}

	e.file("Library/Adele/25/1-02 - Saans.m4a", fixture(t, "b.m4a"))
	os.Remove(filepath.Join(e.s.Root, "Playlists/Shared.m3u8"))
	e.scan()
	e.do("GET", "/api/v1/library?since="+itoa(full.Version), tok, nil, 200, &delta)
	if len(delta.Songs) != 1 || delta.Songs[0].Title != "Saans" {
		t.Errorf("delta songs = %+v", delta.Songs)
	}
	if len(delta.Playlists) != 1 || !delta.Playlists[0].Deleted {
		t.Errorf("delta playlists = %+v, want one tombstone", delta.Playlists)
	}
}

func TestStreamSupportsRange(t *testing.T) {
	e := newEnv(t)
	e.user("vivek", false)
	data := fixture(t, "b.m4a")
	e.file("Library/x.m4a", data)
	e.scan()
	tok := e.login("vivek")

	req, _ := http.NewRequest("GET", e.srv.URL+"/api/v1/stream/1", nil)
	req.Header.Set("Authorization", "Bearer "+tok)
	req.Header.Set("Range", "bytes=100-199")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	body, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	if resp.StatusCode != 206 || !bytes.Equal(body, data[100:200]) {
		t.Errorf("range: status %d, %d bytes", resp.StatusCode, len(body))
	}
	if ct := resp.Header.Get("Content-Type"); ct != "audio/mp4" {
		t.Errorf("content type %q", ct)
	}
	e.do("GET", "/api/v1/stream/999", tok, nil, 404, nil)
}

func TestArtIsScaledAndLyricsPreferLrc(t *testing.T) {
	e := newEnv(t)
	e.user("vivek", false)
	e.file("Library/A/1-01 - Saans.m4a", fixture(t, "b.m4a")) // has embedded lyrics
	e.file("Library/A/1-01 - Saans.lrc", []byte("\uFEFF[00:00.50]Saans (from lrc)\n"))
	img := image.NewRGBA(image.Rect(0, 0, 1000, 800))
	img.Set(0, 0, color.White)
	var buf bytes.Buffer
	png.Encode(&buf, img)
	e.file("Library/A/cover.png", buf.Bytes())
	e.scan()
	tok := e.login("vivek")

	resp := e.do("GET", "/api/v1/art/1?size=300", tok, nil, 200, nil)
	got, _, err := image.Decode(resp.Body)
	if err != nil {
		t.Fatal(err)
	}
	if b := got.Bounds(); b.Dx() != 300 || b.Dy() != 240 {
		t.Errorf("scaled art is %dx%d, want 300x240", b.Dx(), b.Dy())
	}

	var l library.Lyrics
	e.do("GET", "/api/v1/lyrics/1", tok, nil, 200, &l)
	if l.Source != "lrc" || !l.Synced || l.Text != "[00:00.50]Saans (from lrc)\n" {
		t.Errorf("lyrics = %+v", l)
	}
}

func TestRescanIsAdminOnly(t *testing.T) {
	e := newEnv(t)
	e.user("vivek", true)
	e.user("kid", false)
	e.do("POST", "/api/v1/admin/rescan", e.login("kid"), nil, 403, nil)
	e.do("POST", "/api/v1/admin/rescan", e.login("vivek"), nil, 202, nil)
}

func itoa(n int64) string { return strconv.FormatInt(n, 10) }
