package api

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/binary"
	"encoding/json"
	"hash/crc32"
	"image"
	"image/color"
	"image/png"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"net/url"
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
	root, data := t.TempDir(), t.TempDir()
	db, err := store.Open(filepath.Join(data, "dhun.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { db.Close() })
	log := slog.New(slog.NewTextHandler(io.Discard, nil))
	sc := &library.Scanner{DB: db, Root: root, Log: log}
	s := &Server{DB: db, Root: root, DataDir: data, Scanner: sc, Lyrics: &library.LyricsIndex{DB: db, Root: root}, Log: log}
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
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
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

	// Sizes round up to a few steps, so the cache stays small.
	resp := e.do("GET", "/api/v1/art/1?size=200", tok, nil, 200, nil)
	got, _, err := image.Decode(resp.Body)
	if err != nil {
		t.Fatal(err)
	}
	if b := got.Bounds(); b.Dx() != 256 || b.Dy() != 204 {
		t.Errorf("scaled art is %dx%d, want 256x204", b.Dx(), b.Dy())
	}

	var l library.Lyrics
	e.do("GET", "/api/v1/lyrics/1", tok, nil, 200, &l)
	if l.Source != "lrc" || !l.Synced || l.Text != "[00:00.50]Saans (from lrc)\n" {
		t.Errorf("lyrics = %+v", l)
	}
}

func TestLyricsSearchFindsTheLineAndSeesAnEditedLrc(t *testing.T) {
	e := newEnv(t)
	e.user("vivek", false)
	e.file("Library/A/01.mp3", fixture(t, "a.mp3"))
	e.file("Library/A/01.lrc", []byte("[ar:Someone]\n[00:01.00]Hum tere bin ab reh nahi sakte\n[00:05.00]Tere bina kya wajood mera\n"))
	e.file("Library/B/02.opus", fixture(t, "c.opus"))
	e.file("Library/B/02.lrc", []byte("Kabhi kabhi mere dil mein\n"))
	e.scan()
	tok := e.login("vivek")

	search := func(q string) []library.LyricsHit {
		var got struct{ Hits []library.LyricsHit }
		e.do("GET", "/api/v1/search/lyrics?q="+url.QueryEscape(q), tok, nil, 200, &got)
		return got.Hits
	}
	if h := search("tere bina"); len(h) != 1 || h[0].Line != "Tere bina kya wajood mera" {
		t.Errorf("tere bina: %+v", h)
	}
	// Spelling and script are forgiven: Kabhie, and दिल for dil.
	if h := search("kabhie दिल"); len(h) != 1 || h[0].Line != "Kabhi kabhi mere dil mein" {
		t.Errorf("kabhie dil: %+v", h)
	}
	// Words on different lines are not a match.
	if h := search("hum wajood"); len(h) != 0 {
		t.Errorf("hum wajood: %+v", h)
	}

	// An edited .lrc leaves the song's row alone; the refresh still sees it.
	e.file("Library/B/02.lrc", []byte("Ek pyaar ka nagma hai, mauj ki ravani hai\n"))
	if err := e.s.Lyrics.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	if h := search("pyar nagma"); len(h) != 1 {
		t.Errorf("after the edit: %+v", h)
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

// The admin comes from docker-compose.yml on first start, and only then:
// editing the variables later must never touch an existing account.
func TestEnsureAdminOnlyOnFirstStart(t *testing.T) {
	e := newEnv(t)
	ctx := context.Background()
	if _, err := EnsureAdmin(ctx, e.s.DB, "", ""); err == nil {
		t.Error("started with no users and no admin configured")
	}
	if created, err := EnsureAdmin(ctx, e.s.DB, "vivek", "correct horse"); err != nil || !created {
		t.Fatalf("first start: created=%v err=%v", created, err)
	}
	if created, err := EnsureAdmin(ctx, e.s.DB, "vivek", "a new password"); err != nil || created {
		t.Errorf("second start: created=%v err=%v", created, err)
	}
	var me struct{ User userJSON }
	e.do("GET", "/api/v1/me", e.login("vivek"), nil, 200, &me)
	if !me.User.Admin {
		t.Error("the configured user is not the admin")
	}
}

func TestAdminManagesFamilyMembers(t *testing.T) {
	e := newEnv(t)
	e.user("vivek", true)
	admin := e.login("vivek")

	var created struct{ User userJSON }
	e.do("POST", "/api/v1/admin/users", admin, map[string]string{"name": "priya", "password": "correct horse"}, 200, &created)
	if created.User.Name != "priya" || created.User.Admin {
		t.Errorf("created %+v, want member priya", created.User)
	}
	e.do("POST", "/api/v1/admin/users", admin, map[string]string{"name": "Priya", "password": "correct horse"}, 409, nil)
	e.do("POST", "/api/v1/admin/users", admin, map[string]string{"name": "a/b", "password": "correct horse"}, 400, nil)
	e.do("POST", "/api/v1/admin/users", admin, map[string]string{"name": "kid", "password": "short"}, 400, nil)

	priya := e.login("priya")
	e.do("GET", "/api/v1/admin/users", priya, nil, 403, nil)
	e.do("POST", "/api/v1/admin/users", priya, map[string]string{"name": "kid", "password": "correct horse"}, 403, nil)

	var list struct{ Users []adminUserJSON }
	e.do("GET", "/api/v1/admin/users", admin, nil, 200, &list)
	if len(list.Users) != 2 || list.Users[0].Name != "vivek" || list.Users[1].Devices != 1 {
		t.Errorf("users = %+v", list.Users)
	}

	// A forgotten password: reset it, and her old sessions end.
	e.do("PUT", "/api/v1/admin/users/"+itoa(created.User.ID)+"/password", admin, map[string]string{"password": "battery staple"}, 204, nil)
	e.do("GET", "/api/v1/me", priya, nil, 401, nil)
	e.do("POST", "/api/v1/login", "", map[string]string{"username": "priya", "password": "battery staple", "device": "Phone"}, 200, nil)
	e.do("PUT", "/api/v1/admin/users/999/password", admin, map[string]string{"password": "battery staple"}, 404, nil)
}

// The cookie exists for <audio> and <img>. Anything else that arrives with
// only the cookie (another site, or another service on the same host) must
// be refused, and so must a cross-site browser request or a non-JSON body.
func TestCookieOnlyFetchesMedia(t *testing.T) {
	e := newEnv(t)
	e.user("vivek", true)
	e.file("Library/A/Hello.mp3", fixture(t, "a.mp3"))
	e.scan()
	resp := e.do("POST", "/api/v1/login", "", map[string]string{"username": "vivek", "password": "correct horse", "device": "Web"}, 200, nil)
	var cookie *http.Cookie
	for _, c := range resp.Cookies() {
		if c.Name == cookieName {
			cookie = c
		}
	}
	if cookie == nil || !cookie.HttpOnly || cookie.SameSite != http.SameSiteStrictMode {
		t.Fatalf("cookie = %+v", cookie)
	}
	send := func(method, path, contentType, origin string, body string) int {
		req, _ := http.NewRequest(method, e.srv.URL+path, bytes.NewReader([]byte(body)))
		req.AddCookie(cookie)
		if contentType != "" {
			req.Header.Set("Content-Type", contentType)
		}
		if origin != "" {
			req.Header.Set("Origin", origin)
			req.Header.Set("Sec-Fetch-Site", "cross-site")
		}
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		resp.Body.Close()
		if resp.Header.Get("X-Content-Type-Options") != "nosniff" {
			t.Errorf("%s %s: no nosniff header", method, path)
		}
		return resp.StatusCode
	}
	if got := send("GET", "/api/v1/stream/1", "", "", ""); got != 200 {
		t.Errorf("stream with the cookie: %d, want 200", got)
	}
	if got := send("GET", "/api/v1/library", "", "", ""); got != 401 {
		t.Errorf("library with only the cookie: %d, want 401", got)
	}
	if got := send("POST", "/api/v1/admin/users", "application/json", "", `{"name":"x","password":"correct horse"}`); got != 401 {
		t.Errorf("admin call with only the cookie: %d, want 401", got)
	}
	if got := send("POST", "/api/v1/login", "application/json", "http://evil.example", `{"username":"vivek","password":"correct horse","device":"x"}`); got != 403 {
		t.Errorf("cross-site login: %d, want 403", got)
	}
	if got := send("POST", "/api/v1/login", "text/plain", "", `{"username":"vivek","password":"correct horse","device":"x"}`); got != 415 {
		t.Errorf("text/plain body: %d, want 415", got)
	}
}

func TestRepeatedWrongPasswordsWait(t *testing.T) {
	e := newEnv(t)
	e.user("vivek", true)
	bad := map[string]string{"username": "vivek", "password": "wrong", "device": "x"}
	for range freeFailures {
		e.do("POST", "/api/v1/login", "", bad, 401, nil)
	}
	e.do("POST", "/api/v1/login", "", bad, 401, nil) // starts the wait
	resp := e.do("POST", "/api/v1/login", "", map[string]string{"username": "Vivek", "password": "correct horse", "device": "x"}, 429, nil)
	if resp.Header.Get("Retry-After") == "" {
		t.Error("429 without Retry-After")
	}
	// Another name is unaffected.
	e.user("priya", false)
	e.login("priya")
}

// A tiny file claiming a huge image must not be decoded (a gigabyte of
// memory); it is served as it is.
func TestHugeArtIsNotDecoded(t *testing.T) {
	e := newEnv(t)
	e.user("vivek", false)
	e.file("Library/A/Hello.mp3", fixture(t, "a.mp3"))
	var buf bytes.Buffer
	png.Encode(&buf, image.NewGray(image.Rect(0, 0, 1, 1)))
	bomb := buf.Bytes()
	bomb[16], bomb[17], bomb[20], bomb[21] = 0, 0, 0, 0                    // IHDR width and height
	bomb[18], bomb[19], bomb[22], bomb[23] = 0x3e, 0x80, 0x3e, 0x80        // 16000 x 16000
	binary.BigEndian.PutUint32(bomb[29:], crc32.ChecksumIEEE(bomb[12:29])) // a valid header
	if c, _, err := image.DecodeConfig(bytes.NewReader(bomb)); err != nil || c.Width != 16000 {
		t.Fatalf("test image header: %+v %v", c, err)
	}
	e.file("Library/A/cover.png", bomb)
	e.scan()
	resp := e.do("GET", "/api/v1/art/1?size=256", e.login("vivek"), nil, 200, nil)
	if got, _ := io.ReadAll(resp.Body); !bytes.Equal(got, bomb) {
		t.Error("the oversized cover was not served unchanged")
	}
}

// The apps show which server they use; a stranger probing the port is not told.
func TestVersionHeaderOnlyForSignedInClients(t *testing.T) {
	e := newEnv(t)
	e.s.Version = "0.1.2"
	e.user("vivek", true)
	phone := e.login("vivek")
	if v := e.do("GET", "/api/v1/me", phone, nil, 200, nil).Header.Get("Dhun-Version"); v != "0.1.2" {
		t.Errorf("signed in: Dhun-Version = %q, want 0.1.2", v)
	}
	if v := e.do("GET", "/api/v1/me", "", nil, 401, nil).Header.Get("Dhun-Version"); v != "" {
		t.Errorf("signed out: Dhun-Version = %q, want none", v)
	}
}

// Thumbnails come by art key: one per cover however many songs show it, ""
// for a key with nothing to show (so the app stops asking), and a bounded
// batch, since each may mean decoding a large cover.
func TestThumbsByArtKey(t *testing.T) {
	e := newEnv(t)
	e.user("vivek", false)
	e.file("Library/A/1.m4a", fixture(t, "b.m4a"))
	e.file("Library/A/2.m4a", fixture(t, "b.m4a"))
	e.file("Library/B/3.m4a", fixture(t, "b.m4a"))
	var buf bytes.Buffer
	png.Encode(&buf, image.NewRGBA(image.Rect(0, 0, 1000, 800)))
	e.file("Library/A/cover.png", buf.Bytes())
	e.scan()
	tok := e.login("vivek")

	var key string
	e.s.DB.QueryRow(`SELECT art FROM songs WHERE path = 'Library/A/1.m4a'`).Scan(&key)
	var out struct {
		Thumbs map[string]string `json:"thumbs"`
	}
	e.do("POST", "/api/v1/thumbs", tok, map[string]any{"keys": []string{key, "0123456789abcdef"}}, 200, &out)
	if len(out.Thumbs) != 2 || out.Thumbs["0123456789abcdef"] != "" {
		t.Fatalf("thumbs = %v", out.Thumbs)
	}
	data, err := base64.StdEncoding.DecodeString(out.Thumbs[key])
	if err != nil {
		t.Fatal(err)
	}
	img, _, err := image.Decode(bytes.NewReader(data))
	if err != nil {
		t.Fatal(err)
	}
	if b := img.Bounds(); b.Dx() != 128 || b.Dy() != 102 {
		t.Errorf("thumbnail is %dx%d, want 128x102", b.Dx(), b.Dy())
	}

	e.do("POST", "/api/v1/thumbs", tok, map[string]any{"keys": []string{}}, 400, nil)
	e.do("POST", "/api/v1/thumbs", tok, map[string]any{"keys": make([]string, 51)}, 400, nil)
}
