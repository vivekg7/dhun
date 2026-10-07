// Package api is Dhun's HTTP API, the only boundary between the backend and
// the clients (AGENTS.md). Shape and rules: docs/plans/006_api_and_sync.md.
package api

import (
	"compress/gzip"
	"database/sql"
	"encoding/json"
	"errors"
	"log/slog"
	"mime"
	"net/http"
	"strconv"
	"strings"

	"github.com/vivekg7/dhun/server/internal/library"
)

// Server holds what the handlers need.
type Server struct {
	DB      *sql.DB
	Root    string // media root
	DataDir string // Dhun's own state (DHUN_DATA): art cache, kept playlist copies
	Scanner *library.Scanner
	Log     *slog.Logger
	// Rescan starts a scan in the background; set by main.
	Rescan func()
	// Version is the release ("0.1.2", or "dev"), sent to signed-in clients
	// in the Dhun-Version header so the apps can show which server they use.
	Version string

	logins loginFailures
}

type route struct {
	pattern string // "METHOD /path", as http.ServeMux takes it
	handler http.Handler
}

// routes is every endpoint. api/openapi.yaml documents each one;
// TestSpecCoversEveryRoute keeps the two from drifting apart.
func (s *Server) routes() []route {
	return []route{
		{"GET /healthz", http.HandlerFunc(s.healthz)},

		{"POST /api/v1/login", http.HandlerFunc(s.login)},
		{"POST /api/v1/logout", s.authed(s.logout)},
		{"GET /api/v1/me", s.authed(s.me)},
		{"GET /api/v1/devices", s.authed(s.listDevices)},
		{"DELETE /api/v1/devices/{id}", s.authed(s.deleteDevice)},

		{"GET /api/v1/library", s.authed(s.library)},
		{"GET /api/v1/stream/{id}", s.media(s.stream)},
		{"GET /api/v1/art/{id}", s.media(s.art)},
		{"GET /api/v1/lyrics/{id}", s.media(s.lyrics)},

		{"GET /api/v1/sync", s.authed(s.pullSync)},
		{"POST /api/v1/sync", s.authed(s.pushSync)},
		{"GET /api/v1/now-playing", s.authed(s.getNowPlaying)},
		{"GET /api/v1/plays", s.authed(s.playCounts)},

		{"GET /api/v1/admin/users", s.admin(s.listUsers)},
		{"POST /api/v1/admin/users", s.admin(s.createUser)},
		{"PUT /api/v1/admin/users/{id}/password", s.admin(s.setUserPassword)},
		{"POST /api/v1/admin/rescan", s.admin(func(w http.ResponseWriter, r *http.Request, _ session) {
			s.Rescan()
			w.WriteHeader(http.StatusAccepted)
		})},
	}
}

// Handler returns the routed API, behind protections that apply everywhere:
// browsers may not make state-changing requests from another origin (the
// apps send no Origin and are unaffected), and responses are never sniffed
// into something runnable, framed or given a referrer.
func (s *Server) Handler() http.Handler {
	mux := http.NewServeMux()
	for _, rt := range s.routes() {
		mux.Handle(rt.pattern, rt.handler)
	}
	protected := http.NewCrossOriginProtection().Handler(mux)
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		h := w.Header()
		h.Set("X-Content-Type-Options", "nosniff")
		h.Set("Referrer-Policy", "no-referrer")
		h.Set("X-Frame-Options", "DENY")
		if strings.HasPrefix(r.URL.Path, "/api/") {
			h.Set("Content-Security-Policy", "default-src 'none'; frame-ancestors 'none'; sandbox")
		}
		protected.ServeHTTP(w, r)
	})
}

func (s *Server) healthz(w http.ResponseWriter, r *http.Request) {
	if err := s.DB.PingContext(r.Context()); err != nil {
		http.Error(w, "database unavailable", http.StatusServiceUnavailable)
		return
	}
	w.Write([]byte("ok\n"))
}

// apiError is an error with an HTTP status and a stable machine-readable code.
type apiError struct {
	Status  int    `json:"-"`
	Code    string `json:"code"`
	Message string `json:"message"`
}

func (e *apiError) Error() string { return e.Message }

func errNotFound(what string) *apiError {
	return &apiError{http.StatusNotFound, "not_found", what + " not found"}
}

func errBadRequest(msg string) *apiError {
	return &apiError{http.StatusBadRequest, "bad_request", msg}
}

// fail writes err as JSON; anything that is not an *apiError is logged and
// reported as a 500 without its details.
func (s *Server) fail(w http.ResponseWriter, r *http.Request, err error) {
	var ae *apiError
	if !errors.As(err, &ae) {
		s.Log.Error("request failed", "method", r.Method, "path", r.URL.Path, "err", err)
		ae = &apiError{http.StatusInternalServerError, "internal", "internal error"}
	}
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(ae.Status)
	json.NewEncoder(w).Encode(ae)
}

// writeJSON gzips when the client accepts it: the first library sync is
// about 2 MB of JSON and compresses roughly fivefold.
func writeJSON(w http.ResponseWriter, r *http.Request, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.Header().Add("Vary", "Accept-Encoding")
	if !strings.Contains(r.Header.Get("Accept-Encoding"), "gzip") {
		json.NewEncoder(w).Encode(v)
		return
	}
	w.Header().Set("Content-Encoding", "gzip")
	gz := gzip.NewWriter(w)
	json.NewEncoder(gz).Encode(v)
	gz.Close()
}

func readJSON(r *http.Request, v any) error {
	// A JSON body only: a cross-site form cannot send this content type
	// without a CORS preflight, which the server never grants.
	if mt, _, _ := mime.ParseMediaType(r.Header.Get("Content-Type")); mt != "application/json" {
		return &apiError{http.StatusUnsupportedMediaType, "bad_content_type", "send Content-Type: application/json"}
	}
	dec := json.NewDecoder(http.MaxBytesReader(nil, r.Body, 4<<20))
	dec.DisallowUnknownFields()
	if err := dec.Decode(v); err != nil {
		return errBadRequest("invalid JSON body: " + err.Error())
	}
	return nil
}

func pathID(r *http.Request) (int64, error) {
	id, err := strconv.ParseInt(r.PathValue("id"), 10, 64)
	if err != nil || id <= 0 {
		return 0, errBadRequest("invalid id")
	}
	return id, nil
}
