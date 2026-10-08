package api

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"database/sql"
	"encoding/base64"
	"errors"
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"
	"unicode"

	"golang.org/x/crypto/argon2"

	"github.com/vivekg7/dhun/server/internal/store"
)

// Passwords: argon2id with the RFC 9106 "second recommended" parameters.
const (
	argonTime    = 3
	argonMemory  = 64 * 1024 // KiB
	argonThreads = 4
	argonKeyLen  = 32
)

// HashPassword returns an encoded argon2id hash.
func HashPassword(password string) (string, error) {
	salt := make([]byte, 16)
	if _, err := rand.Read(salt); err != nil {
		return "", err
	}
	key := argon2.IDKey([]byte(password), salt, argonTime, argonMemory, argonThreads, argonKeyLen)
	b64 := base64.RawStdEncoding.EncodeToString
	return fmt.Sprintf("argon2id$v=19$m=%d,t=%d,p=%d$%s$%s",
		argonMemory, argonTime, argonThreads, b64(salt), b64(key)), nil
}

func checkPassword(encoded, password string) bool {
	parts := strings.Split(encoded, "$")
	if len(parts) != 5 || parts[0] != "argon2id" {
		return false
	}
	var m uint32
	var t uint32
	var p uint8
	if _, err := fmt.Sscanf(parts[2], "m=%d,t=%d,p=%d", &m, &t, &p); err != nil {
		return false
	}
	salt, err1 := base64.RawStdEncoding.DecodeString(parts[3])
	want, err2 := base64.RawStdEncoding.DecodeString(parts[4])
	if err1 != nil || err2 != nil {
		return false
	}
	got := argon2.IDKey([]byte(password), salt, t, m, p, uint32(len(want)))
	return subtle.ConstantTimeCompare(got, want) == 1
}

// CreateUser adds a user. The admin is created from the environment on first
// start (EnsureAdmin); everyone else by the admin, from the Android app. A
// family has three or four users, so there is no sign-up flow.
func CreateUser(ctx context.Context, db *sql.DB, name, password string, admin bool) error {
	name = strings.TrimSpace(name)
	if name == "" || len(name) > 64 || strings.ContainsAny(name, `/\:*?"<>|`) || strings.ContainsFunc(name, unicode.IsControl) ||
		strings.HasPrefix(name, ".") || strings.HasPrefix(name, "_") || strings.HasPrefix(name, "#") || strings.HasPrefix(name, "@") {
		// The name is also the user's Playlists/<name>/ folder.
		return errBadRequest("user name must be non-empty and usable as a folder name")
	}
	if len(password) < 8 {
		return errBadRequest("password must be at least 8 characters")
	}
	// Compared in Go: SQLite's NOCASE folds ASCII only, and "Émile" and
	// "émile" would otherwise share one Playlists/ folder.
	rows, err := db.QueryContext(ctx, `SELECT name FROM users`)
	if err != nil {
		return err
	}
	defer rows.Close()
	for rows.Next() {
		var other string
		if err := rows.Scan(&other); err != nil {
			return err
		}
		if strings.EqualFold(other, name) {
			return &apiError{http.StatusConflict, "user_exists", fmt.Sprintf("a user named %q already exists", other)}
		}
	}
	if err := rows.Err(); err != nil {
		return err
	}
	hash, err := HashPassword(password)
	if err != nil {
		return err
	}
	_, err = db.ExecContext(ctx, `INSERT INTO users (name, password_hash, is_admin, created_at) VALUES (?, ?, ?, ?)`,
		name, hash, admin, store.Now())
	return err
}

// EnsureAdmin creates the admin from DHUN_ADMIN_USER and DHUN_ADMIN_PASSWORD
// when there are no users yet, so a fresh install is usable straight from
// docker-compose.yml. Once any user exists it does nothing: changing the
// variables later never changes or resets an account.
func EnsureAdmin(ctx context.Context, db *sql.DB, name, password string) (created bool, err error) {
	var users int
	if err := db.QueryRowContext(ctx, `SELECT count(*) FROM users`).Scan(&users); err != nil {
		return false, err
	}
	if users > 0 {
		return false, nil
	}
	if name == "" || password == "" {
		return false, errors.New("no users yet: set DHUN_ADMIN_USER and DHUN_ADMIN_PASSWORD to create the admin")
	}
	if err := CreateUser(ctx, db, name, password, true); err != nil {
		return false, fmt.Errorf("creating the admin: %w", err)
	}
	return true, nil
}

// SetPassword changes a user's password and signs out all their devices.
func SetPassword(ctx context.Context, db *sql.DB, name, password string) error {
	if len(password) < 8 {
		return errBadRequest("password must be at least 8 characters")
	}
	hash, err := HashPassword(password)
	if err != nil {
		return err
	}
	res, err := db.ExecContext(ctx, `UPDATE users SET password_hash = ? WHERE name = ?`, hash, name)
	if err != nil {
		return err
	}
	if n, _ := res.RowsAffected(); n == 0 {
		return errNotFound("user")
	}
	_, err = db.ExecContext(ctx, `DELETE FROM devices WHERE user_id = (SELECT id FROM users WHERE name = ?)`, name)
	return err
}

// session is the authenticated caller.
type session struct {
	UserID   int64
	UserName string
	Admin    bool
	DeviceID int64
}

const cookieName = "dhun_token"

func tokenHash(token string) []byte {
	h := sha256.Sum256([]byte(token))
	return h[:]
}

// authed wraps a handler that needs a signed-in device, which sends
// "Authorization: Bearer <token>".
func (s *Server) authed(h func(http.ResponseWriter, *http.Request, session)) http.Handler {
	return s.auth(h, false)
}

// media is authed for the GET routes a browser loads by URL (<audio>, <img>),
// which cannot set headers: these also accept the token as the HttpOnly
// cookie. Nowhere else does, so a request another site or another service on
// the same host makes with the cookie can at most fetch a song (plan 006).
func (s *Server) media(h func(http.ResponseWriter, *http.Request, session)) http.Handler {
	return s.auth(h, true)
}

func (s *Server) auth(h func(http.ResponseWriter, *http.Request, session), cookie bool) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		token, ok := strings.CutPrefix(r.Header.Get("Authorization"), "Bearer ")
		if !ok && cookie {
			if c, err := r.Cookie(cookieName); err == nil {
				token = c.Value
			}
		}
		if token == "" {
			s.fail(w, r, &apiError{http.StatusUnauthorized, "unauthorized", "sign in first"})
			return
		}
		var sess session
		var lastSeen string
		err := s.DB.QueryRowContext(r.Context(), `SELECT u.id, u.name, u.is_admin, d.id, d.last_seen_at
			FROM devices d JOIN users u ON u.id = d.user_id WHERE d.token_hash = ?`, tokenHash(token)).
			Scan(&sess.UserID, &sess.UserName, &sess.Admin, &sess.DeviceID, &lastSeen)
		if errors.Is(err, sql.ErrNoRows) {
			s.fail(w, r, &apiError{http.StatusUnauthorized, "unauthorized", "this device is signed out"})
			return
		}
		if err != nil {
			s.fail(w, r, err)
			return
		}
		// last_seen is for the devices list, not an audit log: one write per
		// few minutes is plenty, and avoids a write on every stream request.
		if t, err := time.Parse(time.RFC3339, lastSeen); err != nil || time.Since(t) > 5*time.Minute {
			s.DB.ExecContext(r.Context(), `UPDATE devices SET last_seen_at = ? WHERE id = ?`, store.Now(), sess.DeviceID)
		}
		// Signed in only: a stranger probing the port learns nothing.
		w.Header().Set("Dhun-Version", s.Version)
		h(w, r, sess)
	})
}

func (s *Server) admin(h func(http.ResponseWriter, *http.Request, session)) http.Handler {
	return s.authed(func(w http.ResponseWriter, r *http.Request, sess session) {
		if !sess.Admin {
			s.fail(w, r, &apiError{http.StatusForbidden, "forbidden", "admin only"})
			return
		}
		h(w, r, sess)
	})
}

type userJSON struct {
	ID    int64  `json:"id"`
	Name  string `json:"name"`
	Admin bool   `json:"admin"`
}

// Login guards. Every password check costs 64 MiB for argon2, so at most two
// run at once, whatever arrives. A name that keeps failing waits longer and
// longer, and an unknown name costs the same time as a known one, so neither
// guessing nor timing reveals much.
var (
	hashSlots = make(chan struct{}, 2)
	dummyHash = sync.OnceValue(func() string { h, _ := HashPassword("not a password"); return h })
)

const (
	freeFailures = 5                // per name, before any wait
	maxLockout   = 15 * time.Minute // the longest wait after failures
)

type loginFailures struct {
	mu    sync.Mutex
	names map[string]failure
}

type failure struct {
	count int
	until time.Time
}

// wait returns how long name must wait before trying again.
func (l *loginFailures) wait(name string) time.Duration {
	l.mu.Lock()
	defer l.mu.Unlock()
	return time.Until(l.names[name].until)
}

func (l *loginFailures) record(name string, ok bool) {
	l.mu.Lock()
	defer l.mu.Unlock()
	if ok {
		delete(l.names, name)
		return
	}
	if l.names == nil || len(l.names) > 1000 { // bounded: random names cannot grow it
		l.names = map[string]failure{}
	}
	f := l.names[name]
	f.count++
	if f.count > freeFailures {
		// 1 s, 2 s, 4 s, … The shift is capped: past 2^33 s it would overflow
		// into a negative wait, which would end the lockout.
		f.until = time.Now().Add(min(time.Second<<min(f.count-freeFailures-1, 10), maxLockout))
	}
	l.names[name] = f
}

func (s *Server) login(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Username string `json:"username"`
		Password string `json:"password"`
		Device   string `json:"device"` // shown in hand-off: "Continue from <device>"
	}
	if err := readJSON(r, &req); err != nil {
		s.fail(w, r, err)
		return
	}
	if strings.TrimSpace(req.Device) == "" {
		s.fail(w, r, errBadRequest("device name is required"))
		return
	}
	name := strings.ToLower(strings.TrimSpace(req.Username))
	if d := s.logins.wait(name); d > 0 {
		w.Header().Set("Retry-After", strconv.Itoa(int(d.Seconds())+1))
		s.fail(w, r, &apiError{http.StatusTooManyRequests, "too_many_attempts", "too many wrong passwords; try again later"})
		return
	}
	var u userJSON
	var hash string
	err := s.DB.QueryRowContext(r.Context(), `SELECT id, name, is_admin, password_hash FROM users WHERE name = ?`,
		strings.TrimSpace(req.Username)).Scan(&u.ID, &u.Name, &u.Admin, &hash)
	if err != nil && !errors.Is(err, sql.ErrNoRows) {
		s.fail(w, r, err)
		return
	}
	known := err == nil
	if !known {
		hash = dummyHash()
	}
	select {
	case hashSlots <- struct{}{}:
	case <-r.Context().Done():
		return
	}
	ok := checkPassword(hash, req.Password) && known
	<-hashSlots
	s.logins.record(name, ok)
	if !ok {
		s.fail(w, r, &apiError{http.StatusUnauthorized, "bad_credentials", "wrong user name or password"})
		return
	}

	raw := make([]byte, 32)
	if _, err := rand.Read(raw); err != nil {
		s.fail(w, r, err)
		return
	}
	token := base64.RawURLEncoding.EncodeToString(raw)
	now := store.Now()
	var deviceID int64
	if err := s.DB.QueryRowContext(r.Context(), `INSERT INTO devices (user_id, name, token_hash, created_at, last_seen_at)
		VALUES (?, ?, ?, ?, ?) RETURNING id`, u.ID, strings.TrimSpace(req.Device), tokenHash(token), now, now).Scan(&deviceID); err != nil {
		s.fail(w, r, err)
		return
	}
	http.SetCookie(w, &http.Cookie{
		Name: cookieName, Value: token, Path: "/", HttpOnly: true, SameSite: http.SameSiteStrictMode,
		// Behind `tailscale serve` the browser speaks HTTPS; keep the cookie
		// off plain HTTP then.
		Secure: r.TLS != nil || r.Header.Get("X-Forwarded-Proto") == "https",
		MaxAge: 10 * 365 * 24 * 3600, // a device stays signed in until it is removed
	})
	writeJSON(w, r, map[string]any{"token": token, "deviceId": deviceID, "user": u})
}

func (s *Server) logout(w http.ResponseWriter, r *http.Request, sess session) {
	if _, err := s.DB.ExecContext(r.Context(), `DELETE FROM devices WHERE id = ?`, sess.DeviceID); err != nil {
		s.fail(w, r, err)
		return
	}
	http.SetCookie(w, &http.Cookie{Name: cookieName, Path: "/", MaxAge: -1})
	w.WriteHeader(http.StatusNoContent)
}

func (s *Server) me(w http.ResponseWriter, r *http.Request, sess session) {
	writeJSON(w, r, map[string]any{
		"user":     userJSON{sess.UserID, sess.UserName, sess.Admin},
		"deviceId": sess.DeviceID,
	})
}

func (s *Server) listDevices(w http.ResponseWriter, r *http.Request, sess session) {
	rows, err := s.DB.QueryContext(r.Context(),
		`SELECT id, name, created_at, last_seen_at FROM devices WHERE user_id = ? ORDER BY last_seen_at DESC`, sess.UserID)
	if err != nil {
		s.fail(w, r, err)
		return
	}
	defer rows.Close()
	type device struct {
		ID       int64  `json:"id"`
		Name     string `json:"name"`
		Created  string `json:"createdAt"`
		LastSeen string `json:"lastSeenAt"`
		Current  bool   `json:"current"`
	}
	out := []device{}
	for rows.Next() {
		var d device
		if err := rows.Scan(&d.ID, &d.Name, &d.Created, &d.LastSeen); err != nil {
			s.fail(w, r, err)
			return
		}
		d.Current = d.ID == sess.DeviceID
		out = append(out, d)
	}
	writeJSON(w, r, map[string]any{"devices": out})
}

// deleteDevice signs out one of the caller's own devices (a lost phone).
func (s *Server) deleteDevice(w http.ResponseWriter, r *http.Request, sess session) {
	id, err := pathID(r)
	if err != nil {
		s.fail(w, r, err)
		return
	}
	res, err := s.DB.ExecContext(r.Context(), `DELETE FROM devices WHERE id = ? AND user_id = ?`, id, sess.UserID)
	if err != nil {
		s.fail(w, r, err)
		return
	}
	if n, _ := res.RowsAffected(); n == 0 {
		s.fail(w, r, errNotFound("device"))
		return
	}
	w.WriteHeader(http.StatusNoContent)
}
