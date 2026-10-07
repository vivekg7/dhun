package api

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"os"
	"path"
	"slices"
	"strconv"
	"strings"
	"time"

	"github.com/vivekg7/dhun/server/internal/library"
	"github.com/vivekg7/dhun/server/internal/store"
)

// maxQueues is Musicolet's limit, kept on purpose (REQUIREMENTS).
const maxQueues = 20

// opTime is fixed-width, so client times compare correctly as strings in
// "the later change wins" rules; RFC 3339 with variable fractions does not
// ("03.5Z" sorts before "03Z").
const opTime = "2006-01-02T15:04:05.000Z"

// op is one change recorded by a client, possibly while offline. Merge rules
// per type are in docs/plans/006_api_and_sync.md.
type op struct {
	ID   string `json:"id"`
	At   string `json:"at"`
	Type string `json:"type"`

	Queue    string  `json:"queue,omitempty"`
	Playlist string  `json:"playlist,omitempty"` // "123", or "ref:<client ref>" for one created offline
	Ref      string  `json:"ref,omitempty"`      // playlist.create: the client's ref
	Name     string  `json:"name,omitempty"`
	Songs    []int64 `json:"songs,omitempty"`
	Song     int64   `json:"song,omitempty"`
	// Insert/move position: after this song (nil: at the end, 0: at the start).
	// A neighbouring song survives other devices' edits; an index would not.
	After *int64 `json:"after,omitempty"`
	// Playlists may hold a song more than once; these pick which copy (0-based).
	Occurrence      int `json:"occurrence,omitempty"`
	AfterOccurrence int `json:"afterOccurrence,omitempty"`

	PositionMS int64  `json:"positionMs,omitempty"`
	Playing    bool   `json:"playing,omitempty"`
	Shuffle    *bool  `json:"shuffle,omitempty"`
	Repeat     string `json:"repeat,omitempty"`
	MS         int64  `json:"ms,omitempty"` // play: how long it was played
}

type opResult struct {
	ID     string `json:"id"`
	Status string `json:"status"` // applied | duplicate | rejected
	Error  string `json:"error,omitempty"`
}

func rejected(format string, a ...any) *apiError {
	return &apiError{http.StatusUnprocessableEntity, "rejected", fmt.Sprintf(format, a...)}
}

// pushSync applies a batch of operations in order, each in its own
// transaction, then answers with everything changed since the client's
// cursor, so one round trip both pushes and pulls.
//
// A rejected operation is still recorded as applied: retrying it can never
// succeed, and the client must drop it from its outbox.
func (s *Server) pushSync(w http.ResponseWriter, r *http.Request, sess session) {
	var req struct {
		Since int64 `json:"since"`
		Ops   []op  `json:"ops"`
	}
	if err := readJSON(r, &req); err != nil {
		s.fail(w, r, err)
		return
	}
	// A batch is applied up to maxOpsPerPush; the rest get no result, stay in
	// the client's outbox and go with its next sync. That bounds one request's
	// work without making a long offline trip unsendable.
	ops := req.Ops[:min(len(req.Ops), maxOpsPerPush)]
	results := make([]opResult, 0, len(ops))
	for _, o := range ops {
		res, err := s.applyOne(r.Context(), sess, o)
		if err != nil {
			s.fail(w, r, err) // a database failure: the client retries the batch
			return
		}
		results = append(results, res)
	}
	out, err := s.pullSince(r.Context(), sess.UserID, req.Since)
	if err != nil {
		s.fail(w, r, err)
		return
	}
	out["results"] = results
	writeJSON(w, r, out)
}

func (s *Server) pullSync(w http.ResponseWriter, r *http.Request, sess session) {
	since, _ := strconv.ParseInt(r.URL.Query().Get("since"), 10, 64)
	out, err := s.pullSince(r.Context(), sess.UserID, since)
	if err != nil {
		s.fail(w, r, err)
		return
	}
	writeJSON(w, r, out)
}

func (s *Server) applyOne(ctx context.Context, sess session, o op) (opResult, error) {
	res := opResult{ID: o.ID, Status: "applied"}
	if o.ID == "" {
		return opResult{Status: "rejected", Error: "operation has no id"}, nil
	}
	tx, err := s.DB.BeginTx(ctx, nil)
	if err != nil {
		return res, err
	}
	defer tx.Rollback()

	var dup bool
	if err := tx.QueryRowContext(ctx, `SELECT EXISTS (SELECT 1 FROM applied_ops WHERE user_id = ? AND op_id = ?)`,
		sess.UserID, o.ID).Scan(&dup); err != nil {
		return res, err
	}
	if dup {
		return opResult{ID: o.ID, Status: "duplicate"}, nil
	}

	a := &applier{s: s, tx: tx, ctx: ctx, sess: sess, now: store.Now()}
	// The client's time orders its edits against other devices'. A phone
	// whose clock runs ahead would otherwise win every conflict until real
	// time caught up, so a time in the future counts as now.
	at := time.Now()
	if t, err := time.Parse(time.RFC3339Nano, o.At); err == nil && t.Before(at) {
		at = t
	}
	a.at = at.UTC().Format(opTime)
	if err := a.apply(o); err != nil {
		var ae *apiError
		if !errors.As(err, &ae) {
			return res, err
		}
		// Undo whatever the op did before failing, then record it as applied
		// (rejected) in a fresh transaction.
		tx.Rollback()
		if tx, err = s.DB.BeginTx(ctx, nil); err != nil {
			return res, err
		}
		defer tx.Rollback()
		res.Status, res.Error = "rejected", ae.Message
	}
	if _, err := tx.ExecContext(ctx, `INSERT INTO applied_ops (user_id, op_id, applied_at) VALUES (?, ?, ?)`,
		sess.UserID, o.ID, a.now); err != nil {
		return res, err
	}
	return res, tx.Commit()
}

type applier struct {
	s    *Server
	tx   *sql.Tx
	ctx  context.Context
	sess session
	now  string // server time
	at   string // when the client made the change (opTime; falls back to now)

	version int64 // this user's data version for the op, allocated lazily
}

func (a *applier) bump() (int64, error) {
	if a.version == 0 {
		err := a.tx.QueryRowContext(a.ctx,
			`UPDATE users SET data_version = data_version + 1 WHERE id = ? RETURNING data_version`, a.sess.UserID).Scan(&a.version)
		if err != nil {
			return 0, err
		}
	}
	return a.version, nil
}

func (a *applier) apply(o op) error {
	switch o.Type {
	case "queue.create":
		return a.queueCreate(o)
	case "queue.rename", "queue.delete", "queue.insert", "queue.remove", "queue.move",
		"queue.replace", "queue.set_current", "queue.set_mode":
		return a.queueEdit(o)
	case "favorite.set", "favorite.unset":
		return a.favorite(o)
	case "play":
		if err := a.songsExist([]int64{o.Song}); err != nil {
			return err
		}
		_, err := a.tx.ExecContext(a.ctx, `INSERT INTO plays (user_id, song_id, device_id, at, ms_played) VALUES (?, ?, ?, ?, ?)`,
			a.sess.UserID, o.Song, a.sess.DeviceID, a.at, max(o.MS, 0))
		return err
	case "playback.state":
		return a.playbackState(o)
	case "playlist.create":
		return a.playlistCreate(o)
	case "playlist.rename", "playlist.delete", "playlist.insert", "playlist.remove", "playlist.move", "playlist.replace":
		return a.playlistEdit(o)
	}
	return rejected("unknown operation type %q", o.Type)
}

const (
	maxOpsPerPush  = 500
	maxSongsPerOp  = 20000 // a queue of the whole library, with room to grow
	songsPerLookup = 500   // well under SQLite's limit on query parameters
)

func (a *applier) songsExist(ids []int64) error {
	if len(ids) > maxSongsPerOp {
		return rejected("more than %d songs in one operation", maxSongsPerOp)
	}
	uniq := slices.Compact(slices.Sorted(slices.Values(ids)))
	for chunk := range slices.Chunk(uniq, songsPerLookup) {
		q := `SELECT count(*) FROM songs WHERE id IN (?` + strings.Repeat(",?", len(chunk)-1) + `)`
		args := make([]any, len(chunk))
		for i, id := range chunk {
			args[i] = id
		}
		var n int
		if err := a.tx.QueryRowContext(a.ctx, q, args...).Scan(&n); err != nil {
			return err
		}
		if n != len(chunk) {
			return rejected("unknown song id")
		}
	}
	return nil
}

// ---- queues ----

type queueRow struct {
	ID, Name    string
	Songs       []int64
	CurrentSong int64
	PositionMS  int64
	CurrentAt   string
	Shuffle     bool
	Repeat      string
}

func (a *applier) loadQueue(id string) (*queueRow, error) {
	q := &queueRow{ID: id}
	var songs string
	var deleted bool
	err := a.tx.QueryRowContext(a.ctx, `SELECT name, songs, current_song, position_ms, current_at, shuffle, repeat, deleted
		FROM queues WHERE id = ? AND user_id = ?`, id, a.sess.UserID).
		Scan(&q.Name, &songs, &q.CurrentSong, &q.PositionMS, &q.CurrentAt, &q.Shuffle, &q.Repeat, &deleted)
	if errors.Is(err, sql.ErrNoRows) {
		return nil, rejected("no queue %s", id)
	}
	if err != nil {
		return nil, err
	}
	if deleted {
		return nil, rejected("queue %s was deleted", id) // delete wins over later edits
	}
	return q, json.Unmarshal([]byte(songs), &q.Songs)
}

func (a *applier) saveQueue(q *queueRow, touch bool) error {
	v, err := a.bump()
	if err != nil {
		return err
	}
	songs, _ := json.Marshal(q.Songs)
	usedAt := ""
	if touch {
		usedAt = a.now
	}
	_, err = a.tx.ExecContext(a.ctx, `UPDATE queues SET name = ?, songs = ?, current_song = ?, position_ms = ?,
		current_at = ?, shuffle = ?, repeat = ?, used_at = CASE WHEN ? = '' THEN used_at ELSE ? END, version = ?
		WHERE id = ?`, q.Name, string(songs), q.CurrentSong, q.PositionMS, q.CurrentAt, q.Shuffle, q.Repeat,
		usedAt, usedAt, v, q.ID)
	return err
}

// uniqueQueueName adds " (2)", " (3)", … so names stay unique per user.
func (a *applier) uniqueQueueName(name, except string) (string, error) {
	name = strings.TrimSpace(name)
	if name == "" {
		return "", rejected("queue name is empty")
	}
	rows, err := a.tx.QueryContext(a.ctx, `SELECT name FROM queues WHERE user_id = ? AND deleted = 0 AND id != ?`,
		a.sess.UserID, except)
	if err != nil {
		return "", err
	}
	taken := map[string]bool{}
	for rows.Next() {
		var n string
		rows.Scan(&n)
		taken[strings.ToLower(n)] = true
	}
	rows.Close()
	candidate := name
	for i := 2; taken[strings.ToLower(candidate)]; i++ {
		candidate = fmt.Sprintf("%s (%d)", name, i)
	}
	return candidate, nil
}

func dedupe(ids []int64) []int64 {
	seen := map[int64]bool{}
	out := make([]int64, 0, len(ids))
	for _, id := range ids {
		if !seen[id] {
			seen[id] = true
			out = append(out, id)
		}
	}
	return out
}

func (a *applier) queueCreate(o op) error {
	if o.Queue == "" {
		return rejected("queue id is required")
	}
	var exists bool
	if err := a.tx.QueryRowContext(a.ctx, `SELECT EXISTS (SELECT 1 FROM queues WHERE id = ?)`, o.Queue).Scan(&exists); err != nil {
		return err
	}
	if exists {
		return rejected("queue %s already exists", o.Queue)
	}
	songs := dedupe(o.Songs) // a queue never holds a song twice
	if err := a.songsExist(songs); err != nil {
		return err
	}
	name, err := a.uniqueQueueName(o.Name, "")
	if err != nil {
		return err
	}
	v, err := a.bump()
	if err != nil {
		return err
	}
	// Past 20 queues the least recently used one goes, as in Musicolet.
	for {
		var n int
		if err := a.tx.QueryRowContext(a.ctx, `SELECT count(*) FROM queues WHERE user_id = ? AND deleted = 0`, a.sess.UserID).Scan(&n); err != nil {
			return err
		}
		if n < maxQueues {
			break
		}
		if _, err := a.tx.ExecContext(a.ctx, `UPDATE queues SET deleted = 1, version = ? WHERE id = (
			SELECT id FROM queues WHERE user_id = ? AND deleted = 0 ORDER BY used_at, created_at LIMIT 1)`, v, a.sess.UserID); err != nil {
			return err
		}
	}
	current := o.Song
	if current == 0 && len(songs) > 0 {
		current = songs[0]
	}
	data, _ := json.Marshal(songs)
	_, err = a.tx.ExecContext(a.ctx, `INSERT INTO queues (id, user_id, name, songs, current_song, position_ms, current_at,
		created_at, used_at, version) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		o.Queue, a.sess.UserID, name, string(data), current, max(o.PositionMS, 0), a.at, a.now, a.now, v)
	return err
}

// insertAfter puts songs after `after` (nil: end, 0: start, unknown: end),
// moving any that are already in the list rather than duplicating them.
func insertAfter(list, songs []int64, after *int64) []int64 {
	songs = dedupe(songs)
	moving := map[int64]bool{}
	for _, id := range songs {
		moving[id] = true
	}
	rest := make([]int64, 0, len(list))
	for _, id := range list {
		if !moving[id] {
			rest = append(rest, id)
		}
	}
	at := len(rest)
	if after != nil {
		if *after == 0 {
			at = 0
		} else if i := slices.Index(rest, *after); i >= 0 {
			at = i + 1
		}
	}
	return slices.Insert(rest, at, songs...)
}

func (a *applier) queueEdit(o op) error {
	q, err := a.loadQueue(o.Queue)
	if err != nil {
		return err
	}
	touch := false
	switch o.Type {
	case "queue.rename":
		if q.Name, err = a.uniqueQueueName(o.Name, q.ID); err != nil {
			return err
		}
	case "queue.delete":
		v, err := a.bump()
		if err != nil {
			return err
		}
		_, err = a.tx.ExecContext(a.ctx, `UPDATE queues SET deleted = 1, version = ? WHERE id = ?`, v, q.ID)
		return err
	case "queue.insert", "queue.move":
		songs := o.Songs
		if o.Type == "queue.move" {
			songs = []int64{o.Song}
			if !slices.Contains(q.Songs, o.Song) {
				return rejected("song %d is not in the queue", o.Song)
			}
		}
		if err := a.songsExist(songs); err != nil {
			return err
		}
		q.Songs = insertAfter(q.Songs, songs, o.After)
	case "queue.remove":
		drop := map[int64]bool{}
		for _, id := range o.Songs {
			drop[id] = true
		}
		q.Songs = slices.DeleteFunc(q.Songs, func(id int64) bool { return drop[id] })
	case "queue.replace": // a sort or shuffle: the whole order, last writer wins
		songs := dedupe(o.Songs)
		if err := a.songsExist(songs); err != nil {
			return err
		}
		q.Songs = songs
	case "queue.set_current":
		if a.at < q.CurrentAt {
			return nil // an older position than the one we have: ignore
		}
		q.CurrentSong, q.PositionMS, q.CurrentAt = o.Song, max(o.PositionMS, 0), a.at
		touch = true
	case "queue.set_mode":
		if o.Shuffle != nil {
			q.Shuffle = *o.Shuffle
		}
		if o.Repeat != "" {
			if o.Repeat != "off" && o.Repeat != "queue" && o.Repeat != "song" {
				return rejected("repeat must be off, queue or song")
			}
			q.Repeat = o.Repeat
		}
	}
	return a.saveQueue(q, touch)
}

// ---- favorites and playback ----

func (a *applier) favorite(o op) error {
	if err := a.songsExist([]int64{o.Song}); err != nil {
		return err
	}
	var at string
	err := a.tx.QueryRowContext(a.ctx, `SELECT at FROM favorites WHERE user_id = ? AND song_id = ?`, a.sess.UserID, o.Song).Scan(&at)
	if err != nil && !errors.Is(err, sql.ErrNoRows) {
		return err
	}
	if err == nil && a.at < at {
		return nil // the later set/unset wins
	}
	v, err := a.bump()
	if err != nil {
		return err
	}
	_, err = a.tx.ExecContext(a.ctx, `INSERT INTO favorites (user_id, song_id, at, deleted, version) VALUES (?, ?, ?, ?, ?)
		ON CONFLICT (user_id, song_id) DO UPDATE SET at = excluded.at, deleted = excluded.deleted, version = excluded.version`,
		a.sess.UserID, o.Song, a.at, o.Type == "favorite.unset", v)
	return err
}

func (a *applier) playbackState(o op) error {
	var at string
	err := a.tx.QueryRowContext(a.ctx, `SELECT at FROM now_playing WHERE user_id = ?`, a.sess.UserID).Scan(&at)
	if err != nil && !errors.Is(err, sql.ErrNoRows) {
		return err
	}
	if err == nil && a.at < at {
		return nil
	}
	var device string
	if err := a.tx.QueryRowContext(a.ctx, `SELECT name FROM devices WHERE id = ?`, a.sess.DeviceID).Scan(&device); err != nil {
		return err
	}
	v, err := a.bump()
	if err != nil {
		return err
	}
	_, err = a.tx.ExecContext(a.ctx, `INSERT INTO now_playing (user_id, device_id, device_name, queue_id, song_id, position_ms,
		playing, at, version) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
		ON CONFLICT (user_id) DO UPDATE SET device_id = excluded.device_id, device_name = excluded.device_name,
		queue_id = excluded.queue_id, song_id = excluded.song_id, position_ms = excluded.position_ms,
		playing = excluded.playing, at = excluded.at, version = excluded.version`,
		a.sess.UserID, a.sess.DeviceID, device, o.Queue, o.Song, max(o.PositionMS, 0), o.Playing, a.at, v)
	return err
}

// ---- playlists: the .m3u8 files are the truth (plan 005) ----

type playlistRow struct {
	ID      int64
	Path    string
	Name    string
	Owner   sql.NullInt64
	entries []library.WriteEntry
	songIDs []int64 // parallel to entries; 0 for an unresolved entry
}

func (a *applier) loadPlaylist(ref string) (*playlistRow, error) {
	p := &playlistRow{}
	var deleted bool
	var err error
	if r, ok := strings.CutPrefix(ref, "ref:"); ok {
		err = a.tx.QueryRowContext(a.ctx, `SELECT id, path, name, owner_id, deleted FROM playlists WHERE client_ref = ?`, r).
			Scan(&p.ID, &p.Path, &p.Name, &p.Owner, &deleted)
	} else {
		id, perr := strconv.ParseInt(ref, 10, 64)
		if perr != nil {
			return nil, rejected("bad playlist reference %q", ref)
		}
		err = a.tx.QueryRowContext(a.ctx, `SELECT id, path, name, owner_id, deleted FROM playlists WHERE id = ?`, id).
			Scan(&p.ID, &p.Path, &p.Name, &p.Owner, &deleted)
	}
	if errors.Is(err, sql.ErrNoRows) || deleted {
		return nil, rejected("no playlist %s", ref)
	}
	if err != nil {
		return nil, err
	}
	// Your own playlists, or the shared ones if you are the admin.
	if !(p.Owner.Valid && p.Owner.Int64 == a.sess.UserID) && !(!p.Owner.Valid && a.sess.Admin) {
		return nil, rejected("playlist %s is not yours to edit", ref)
	}
	// The file is the truth: pick up changes made to it outside Dhun first.
	err = library.RefreshPlaylist(a.ctx, a.tx, a.s.Root, p.Path, p.ID, p.Owner, func() (int64, error) {
		return store.NextLibraryVersion(a.ctx, a.tx)
	})
	if errors.Is(err, os.ErrNotExist) {
		return nil, rejected("playlist %s was removed from the Playlists folder", ref)
	}
	if err == nil { // the file may also have been renamed inside (#PLAYLIST:)
		err = a.tx.QueryRowContext(a.ctx, `SELECT name FROM playlists WHERE id = ?`, p.ID).Scan(&p.Name)
	}
	if err != nil {
		return nil, err
	}
	rows, err := a.tx.QueryContext(a.ctx, `SELECT COALESCE(i.song_id, 0), i.raw_path, i.title, i.duration_s, i.lines_before,
		COALESCE(s.path, ''), COALESCE(s.title, ''), COALESCE(s.artist, ''), COALESCE(s.duration_ms, 0)
		FROM playlist_items i LEFT JOIN songs s ON s.id = i.song_id WHERE i.playlist_id = ? ORDER BY i.pos`, p.ID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	for rows.Next() {
		var id, durMS int64
		var raw, title, before, sPath, sTitle, sArtist string
		var dur int
		if err := rows.Scan(&id, &raw, &title, &dur, &before, &sPath, &sTitle, &sArtist, &durMS); err != nil {
			return nil, err
		}
		// An edit changes only what it is about: every other entry is written
		// back exactly as it was in the file, its path rewritten only if the
		// song has moved since. Only new entries get an #EXTINF from the tags.
		e := library.WriteEntry{Raw: raw, Title: title, DurationS: dur}
		if id != 0 && library.Resolve(p.Path, raw) != library.PathKey(sPath) {
			e.SongPath = sPath
		}
		if before != "" {
			e.Before = strings.Split(before, "\n")
		}
		p.entries = append(p.entries, e)
		p.songIDs = append(p.songIDs, id)
	}
	return p, rows.Err()
}

func songEntry(path, title, artist string, durMS int64) library.WriteEntry {
	t := title
	if artist != "" {
		t = artist + " - " + title
	}
	return library.WriteEntry{SongPath: path, Title: t, DurationS: int(durMS / 1000)}
}

func (a *applier) songEntries(ids []int64) ([]library.WriteEntry, error) {
	if err := a.songsExist(ids); err != nil {
		return nil, err
	}
	out := make([]library.WriteEntry, len(ids))
	for i, id := range ids {
		var p, t, ar string
		var d int64
		if err := a.tx.QueryRowContext(a.ctx, `SELECT path, title, artist, duration_ms FROM songs WHERE id = ?`, id).Scan(&p, &t, &ar, &d); err != nil {
			return nil, err
		}
		out[i] = songEntry(p, t, ar, d)
	}
	return out, nil
}

// indexOf finds the n-th (0-based) occurrence of song in ids.
func indexOf(ids []int64, song int64, n int) int {
	for i, id := range ids {
		if id == song {
			if n == 0 {
				return i
			}
			n--
		}
	}
	return -1
}

func (a *applier) savePlaylist(p *playlistRow, rel string) error {
	v, err := store.NextLibraryVersion(a.ctx, a.tx)
	if err != nil {
		return err
	}
	_, err = library.SavePlaylist(a.ctx, a.tx, a.s.Root, a.s.DataDir, rel, p.ID, p.Owner, p.Name, p.entries, v)
	return err
}

func (a *applier) playlistCreate(o op) error {
	if o.Ref == "" {
		return rejected("playlist ref is required")
	}
	var exists bool
	if err := a.tx.QueryRowContext(a.ctx, `SELECT EXISTS (SELECT 1 FROM playlists WHERE client_ref = ?)`, o.Ref).Scan(&exists); err != nil {
		return err
	}
	if exists {
		return rejected("playlist ref %s already exists", o.Ref)
	}
	entries, err := a.songEntries(o.Songs)
	if err != nil {
		return err
	}
	rel, err := library.PlaylistRel(a.s.Root, path.Join(library.PlaylistsDir, a.sess.UserName), o.Name, "")
	if err != nil {
		return rejected("%v", err)
	}
	p := &playlistRow{Name: strings.TrimSpace(o.Name), Owner: sql.NullInt64{Int64: a.sess.UserID, Valid: true}, entries: entries}
	v, err := store.NextLibraryVersion(a.ctx, a.tx)
	if err != nil {
		return err
	}
	id, err := library.SavePlaylist(a.ctx, a.tx, a.s.Root, a.s.DataDir, rel, 0, p.Owner, p.Name, p.entries, v)
	if err != nil {
		return err
	}
	_, err = a.tx.ExecContext(a.ctx, `UPDATE playlists SET client_ref = ? WHERE id = ?`, o.Ref, id)
	return err
}

func (a *applier) playlistEdit(o op) error {
	p, err := a.loadPlaylist(o.Playlist)
	if err != nil {
		return err
	}
	rel := p.Path
	switch o.Type {
	case "playlist.delete":
		// Copied to the data folder first, never deleted outright.
		if err := library.TrashPlaylist(a.s.Root, a.s.DataDir, p.Path, time.Now()); err != nil {
			return err
		}
		v, err := store.NextLibraryVersion(a.ctx, a.tx)
		if err != nil {
			return err
		}
		if _, err := a.tx.ExecContext(a.ctx, `UPDATE playlists SET deleted = 1, version = ? WHERE id = ?`, v, p.ID); err != nil {
			return err
		}
		_, err = a.tx.ExecContext(a.ctx, `DELETE FROM playlist_items WHERE playlist_id = ?`, p.ID)
		return err
	case "playlist.rename":
		name := strings.TrimSpace(o.Name)
		if name == "" {
			return rejected("playlist name is empty")
		}
		if rel, err = library.PlaylistRel(a.s.Root, path.Dir(p.Path), name, p.Path); err != nil {
			return rejected("%v", err)
		}
		if err := library.RenamePlaylist(a.s.Root, a.s.DataDir, p.Path, rel, time.Now()); err != nil {
			return err
		}
		p.Name = name
	case "playlist.insert":
		entries, err := a.songEntries(o.Songs)
		if err != nil {
			return err
		}
		at := len(p.entries)
		if o.After != nil {
			if *o.After == 0 {
				at = 0
			} else if i := indexOf(p.songIDs, *o.After, o.AfterOccurrence); i >= 0 {
				at = i + 1
			}
		}
		p.entries = slices.Insert(p.entries, at, entries...)
	case "playlist.remove":
		i := indexOf(p.songIDs, o.Song, o.Occurrence)
		if i < 0 {
			return nil // already gone: removing twice is the same as once
		}
		p.entries = slices.Delete(p.entries, i, i+1)
	case "playlist.move":
		i := indexOf(p.songIDs, o.Song, o.Occurrence)
		if i < 0 {
			return rejected("song %d is not in the playlist", o.Song)
		}
		e := p.entries[i]
		p.entries = slices.Delete(p.entries, i, i+1)
		ids := slices.Delete(slices.Clone(p.songIDs), i, i+1)
		at := len(p.entries)
		if o.After != nil {
			if *o.After == 0 {
				at = 0
			} else if j := indexOf(ids, *o.After, o.AfterOccurrence); j >= 0 {
				at = j + 1
			}
		}
		p.entries = slices.Insert(p.entries, at, e)
	case "playlist.replace":
		if p.entries, err = a.songEntries(o.Songs); err != nil {
			return err
		}
	}
	return a.savePlaylist(p, rel)
}

// ---- pull ----

func (s *Server) pullSince(ctx context.Context, userID, since int64) (map[string]any, error) {
	tx, err := s.DB.BeginTx(ctx, &sql.TxOptions{ReadOnly: true})
	if err != nil {
		return nil, err
	}
	defer tx.Rollback()

	var version int64
	if err := tx.QueryRowContext(ctx, `SELECT data_version FROM users WHERE id = ?`, userID).Scan(&version); err != nil {
		return nil, err
	}

	type queueJSON struct {
		ID          string          `json:"id"`
		Deleted     bool            `json:"deleted,omitempty"`
		Name        string          `json:"name,omitempty"`
		Songs       json.RawMessage `json:"songs,omitempty"`
		CurrentSong int64           `json:"currentSong,omitempty"`
		PositionMS  int64           `json:"positionMs,omitempty"`
		Shuffle     bool            `json:"shuffle,omitempty"`
		Repeat      string          `json:"repeat,omitempty"`
		UsedAt      string          `json:"usedAt,omitempty"`
	}
	queues := []queueJSON{}
	rows, err := tx.QueryContext(ctx, `SELECT id, deleted, name, songs, current_song, position_ms, shuffle, repeat, used_at
		FROM queues WHERE user_id = ? AND version > ? AND NOT (deleted AND ? = 0) ORDER BY used_at DESC`, userID, since, since)
	if err != nil {
		return nil, err
	}
	for rows.Next() {
		var q queueJSON
		var songs string
		if err := rows.Scan(&q.ID, &q.Deleted, &q.Name, &songs, &q.CurrentSong, &q.PositionMS, &q.Shuffle, &q.Repeat, &q.UsedAt); err != nil {
			rows.Close()
			return nil, err
		}
		if q.Deleted {
			q = queueJSON{ID: q.ID, Deleted: true}
		} else {
			q.Songs = json.RawMessage(songs)
		}
		queues = append(queues, q)
	}
	rows.Close()

	type favJSON struct {
		Song    int64 `json:"song"`
		Deleted bool  `json:"deleted,omitempty"`
	}
	favs := []favJSON{}
	rows, err = tx.QueryContext(ctx, `SELECT song_id, deleted FROM favorites WHERE user_id = ? AND version > ?
		AND NOT (deleted AND ? = 0)`, userID, since, since)
	if err != nil {
		return nil, err
	}
	for rows.Next() {
		var f favJSON
		if err := rows.Scan(&f.Song, &f.Deleted); err != nil {
			rows.Close()
			return nil, err
		}
		favs = append(favs, f)
	}
	rows.Close()

	np, err := nowPlaying(ctx, tx, userID, since)
	if err != nil {
		return nil, err
	}
	return map[string]any{"version": version, "queues": queues, "favorites": favs, "nowPlaying": np}, nil
}

type nowPlayingJSON struct {
	DeviceID   int64  `json:"deviceId"`
	DeviceName string `json:"deviceName"`
	Queue      string `json:"queue"`
	Song       int64  `json:"song"`
	PositionMS int64  `json:"positionMs"`
	Playing    bool   `json:"playing"`
	At         string `json:"at"`
}

type querier interface {
	QueryRowContext(ctx context.Context, query string, args ...any) *sql.Row
}

func nowPlaying(ctx context.Context, q querier, userID, since int64) (*nowPlayingJSON, error) {
	var np nowPlayingJSON
	err := q.QueryRowContext(ctx, `SELECT device_id, device_name, queue_id, song_id, position_ms, playing, at
		FROM now_playing WHERE user_id = ? AND version > ?`, userID, since).
		Scan(&np.DeviceID, &np.DeviceName, &np.Queue, &np.Song, &np.PositionMS, &np.Playing, &np.At)
	if errors.Is(err, sql.ErrNoRows) {
		return nil, nil
	}
	return &np, err
}

// getNowPlaying is what an app checks when it comes to the foreground, to
// offer "Continue from Phone at 2:13" (resume hand-off, plan 002).
func (s *Server) getNowPlaying(w http.ResponseWriter, r *http.Request, sess session) {
	np, err := nowPlaying(r.Context(), s.DB, sess.UserID, 0)
	if err != nil {
		s.fail(w, r, err)
		return
	}
	writeJSON(w, r, map[string]any{"nowPlaying": np})
}

// playCounts returns the caller's play count and last play per song, for
// sorting by most / recently played on the client.
func (s *Server) playCounts(w http.ResponseWriter, r *http.Request, sess session) {
	rows, err := s.DB.QueryContext(r.Context(), `SELECT song_id, count(*), max(at) FROM plays WHERE user_id = ?
		GROUP BY song_id`, sess.UserID)
	if err != nil {
		s.fail(w, r, err)
		return
	}
	defer rows.Close()
	type pc struct {
		Song       int64  `json:"song"`
		Count      int    `json:"count"`
		LastPlayed string `json:"lastPlayedAt"`
	}
	out := []pc{}
	for rows.Next() {
		var p pc
		if err := rows.Scan(&p.Song, &p.Count, &p.LastPlayed); err != nil {
			s.fail(w, r, err)
			return
		}
		out = append(out, p)
	}
	writeJSON(w, r, map[string]any{"plays": out})
}
