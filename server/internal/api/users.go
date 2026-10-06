package api

import (
	"database/sql"
	"errors"
	"net/http"
	"strings"
)

// Family members are managed by the admin. Only the admin is an admin: the
// first account comes from docker-compose.yml (EnsureAdmin), and accounts
// created here are members (docs/REQUIREMENTS.md, "Roles").

type adminUserJSON struct {
	userJSON
	Created  string `json:"createdAt"`
	Devices  int    `json:"devices"`
	LastSeen string `json:"lastSeenAt,omitempty"`
}

func (s *Server) listUsers(w http.ResponseWriter, r *http.Request, _ session) {
	rows, err := s.DB.QueryContext(r.Context(), `SELECT u.id, u.name, u.is_admin, u.created_at,
			count(d.id), coalesce(max(d.last_seen_at), '')
		FROM users u LEFT JOIN devices d ON d.user_id = u.id
		GROUP BY u.id ORDER BY u.is_admin DESC, u.name`)
	if err != nil {
		s.fail(w, r, err)
		return
	}
	defer rows.Close()
	out := []adminUserJSON{}
	for rows.Next() {
		var u adminUserJSON
		if err := rows.Scan(&u.ID, &u.Name, &u.Admin, &u.Created, &u.Devices, &u.LastSeen); err != nil {
			s.fail(w, r, err)
			return
		}
		out = append(out, u)
	}
	if err := rows.Err(); err != nil {
		s.fail(w, r, err)
		return
	}
	writeJSON(w, r, map[string]any{"users": out})
}

func (s *Server) createUser(w http.ResponseWriter, r *http.Request, _ session) {
	var req struct {
		Name     string `json:"name"`
		Password string `json:"password"`
	}
	if err := readJSON(r, &req); err != nil {
		s.fail(w, r, err)
		return
	}
	if err := CreateUser(r.Context(), s.DB, req.Name, req.Password, false); err != nil {
		s.fail(w, r, err)
		return
	}
	var u userJSON
	if err := s.DB.QueryRowContext(r.Context(), `SELECT id, name, is_admin FROM users WHERE name = ?`,
		strings.TrimSpace(req.Name)).Scan(&u.ID, &u.Name, &u.Admin); err != nil {
		s.fail(w, r, err)
		return
	}
	writeJSON(w, r, map[string]any{"user": u})
}

// setUserPassword is for a family member who forgot theirs. It signs out all
// of that user's devices.
func (s *Server) setUserPassword(w http.ResponseWriter, r *http.Request, _ session) {
	id, err := pathID(r)
	if err != nil {
		s.fail(w, r, err)
		return
	}
	var req struct {
		Password string `json:"password"`
	}
	if err := readJSON(r, &req); err != nil {
		s.fail(w, r, err)
		return
	}
	var name string
	err = s.DB.QueryRowContext(r.Context(), `SELECT name FROM users WHERE id = ?`, id).Scan(&name)
	if errors.Is(err, sql.ErrNoRows) {
		err = errNotFound("user")
	}
	if err == nil {
		err = SetPassword(r.Context(), s.DB, name, req.Password)
	}
	if err != nil {
		s.fail(w, r, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}
