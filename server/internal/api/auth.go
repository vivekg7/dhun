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
	"strings"
	"time"

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

// CreateUser adds a user; used by the `dhun user add` command. Users are
// managed from the command line: a family has three or four of them.
func CreateUser(ctx context.Context, db *sql.DB, name, password string, admin bool) error {
	name = strings.TrimSpace(name)
	if name == "" || strings.ContainsAny(name, `/\:*?"<>|`) || strings.HasPrefix(name, ".") {
		// The name is also the user's Playlists/<name>/ folder.
		return errors.New("user name must be non-empty and usable as a folder name")
	}
	if len(password) < 8 {
		return errors.New("password must be at least 8 characters")
	}
	hash, err := HashPassword(password)
	if err != nil {
		return err
	}
	_, err = db.ExecContext(ctx, `INSERT INTO users (name, password_hash, is_admin, created_at) VALUES (?, ?, ?, ?)`,
		name, hash, admin, store.Now())
	return err
}

// SetPassword changes a user's password and signs out all their devices.
func SetPassword(ctx context.Context, db *sql.DB, name, password string) error {
	if len(password) < 8 {
		return errors.New("password must be at least 8 characters")
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
		return fmt.Errorf("no user named %q", name)
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

// authed wraps a handler that needs a signed-in device. Apps send
// "Authorization: Bearer <token>"; the web client sends the same token as an
// HttpOnly cookie, because <audio> cannot set headers (plan 006).
func (s *Server) authed(h func(http.ResponseWriter, *http.Request, session)) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		token := strings.TrimPrefix(r.Header.Get("Authorization"), "Bearer ")
		if token == "" || token == r.Header.Get("Authorization") {
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
	var u userJSON
	var hash string
	err := s.DB.QueryRowContext(r.Context(), `SELECT id, name, is_admin, password_hash FROM users WHERE name = ?`,
		strings.TrimSpace(req.Username)).Scan(&u.ID, &u.Name, &u.Admin, &hash)
	if err != nil && !errors.Is(err, sql.ErrNoRows) {
		s.fail(w, r, err)
		return
	}
	if err != nil || !checkPassword(hash, req.Password) {
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
		Name: cookieName, Value: token, Path: "/", HttpOnly: true, SameSite: http.SameSiteLaxMode,
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
