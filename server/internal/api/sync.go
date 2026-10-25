package api

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"iter"
	"net/http"
	"os"
	"path"
	"slices"
	"strconv"
	"strings"
	"time"
	"unicode/utf8"

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
	// play: one listen, however short (docs/plans/008_listening_history.md).
	// The op's at is when it started; ms is time actually heard.
	MS        int64  `json:"ms,omitempty"`
	EndedAt   string `json:"endedAt,omitempty"`
	FromMS    int64  `json:"fromMs,omitempty"`
	ToMS      int64  `json:"toMs,omitempty"`
	End       string `json:"end,omitempty"`
	Source    string `json:"source,omitempty"`
	UTCOffset *int   `json:"utcOffset,omitempty"` // minutes

	Value json.RawMessage `json:"value,omitempty"` // setting.set

	Bookmark string `json:"bookmark,omitempty"` // bookmark.set/unset: the client's id
	Note     string `json:"note,omitempty"`
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

// write runs q, a change to the user's data, with the op's data version as
// its first argument: allocated by the op's first write, shared by the rest.
func (a *applier) write(q string, args ...any) error {
	if a.version == 0 {
		err := a.tx.QueryRowContext(a.ctx,
			`UPDATE users SET data_version = data_version + 1 WHERE id = ? RETURNING data_version`, a.sess.UserID).Scan(&a.version)
		if err != nil {
			return err
		}
	}
	_, err := a.tx.ExecContext(a.ctx, q, append([]any{a.version}, args...)...)
	return err
}

func (a *applier) apply(o op) error {
	switch o.Type {
	case "queue.create":
		return a.queueCreate(o)
	case "queue.rename", "queue.delete", "queue.insert", "queue.remove", "queue.move",
		"queue.replace", "queue.set_current", "queue.set_mode":
		return a.queueEdit(o)
	case "favorite.set", "favorite.unset":
		return a.mark("favorites", o.Song, o.Type == "favorite.unset", a.at)
	case "listen_later.add", "listen_later.remove":
		return a.mark("listen_later", o.Song, o.Type == "listen_later.remove", a.at)
	case "played.set", "played.unset":
		return a.mark("played", o.Song, o.Type == "played.unset", a.at)
	case "play":
		return a.play(o)
	case "playback.state":
		return a.playbackState(o)
	case "resume.set", "resume.unset":
		return a.resume(o)
	case "setting.set":
		return a.setting(o)
	case "bookmark.set", "bookmark.unset":
		return a.bookmark(o)
	case "playlist.create":
		return a.playlistCreate(o)
	case "playlist.rename", "playlist.delete", "playlist.insert", "playlist.remove", "playlist.move", "playlist.replace":
		return a.playlistEdit(o)
	}
	return rejected("unknown operation type %q", o.Type)
}

// play appends one listen to the log. Nothing is rejected for being odd: the
// listen happened, and a recommender is better off with it.
func (a *applier) play(o op) error {
	if err := a.songsExist([]int64{o.Song}); err != nil {
		return err
	}
	// A listen ends after it starts and, like at, not in the future.
	ended := a.at
	if t, err := time.Parse(time.RFC3339Nano, o.EndedAt); err == nil {
		if now := time.Now(); t.After(now) {
			t = now
		}
		ended = max(ended, t.UTC().Format(opTime))
	}
	offset := o.UTCOffset
	if offset != nil && (*offset < -14*60 || *offset > 14*60) {
		offset = nil
	}
	_, err := a.tx.ExecContext(a.ctx, `INSERT INTO plays (user_id, song_id, device_id, at, ms_played,
		ended_at, utc_offset, from_ms, to_ms, end_reason, source, queue_id, shuffle)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		a.sess.UserID, o.Song, a.sess.DeviceID, a.at, max(o.MS, 0),
		ended, offset, max(o.FromMS, 0), max(o.ToMS, 0), label(o.End, 32), label(o.Source, 200),
		label(o.Queue, 200), o.Shuffle)
	if err != nil {
		return err
	}
	if err := a.finishListenLater(o, ended); err != nil {
		return err
	}
	return a.markPlayed(o, ended)
}

// playedPercent is how far into an episode or a book's file a listen must
// reach for it to be marked played: Listen Later's threshold (plan 031).
const playedPercent = 90

// markPlayed marks a podcast or audiobook file played once a listen reached
// far enough into it. Music has play counts instead.
func (a *applier) markPlayed(o op, ended string) error {
	var kind string
	var dur int64
	var played bool
	err := a.tx.QueryRowContext(a.ctx, `SELECT s.kind, s.duration_ms, COALESCE(NOT p.deleted, 0) FROM songs s
		LEFT JOIN played p ON p.song_id = s.id AND p.user_id = ? WHERE s.id = ?`, a.sess.UserID, o.Song).Scan(&kind, &dur, &played)
	if err != nil || kind == library.KindMusic || played || dur <= 0 || o.ToMS*100 < dur*playedPercent {
		return err
	}
	// As of the listen's end: marking it unplayed later still wins.
	return a.mark("played", o.Song, false, ended)
}

// finishListenLater takes a song off Listen Later once a listen reached far
// enough into it: by position, so a podcast heard over several sittings
// counts (plan 010). Both rules are the user's synced settings.
func (a *applier) finishListenLater(o op, ended string) error {
	on := readSetting(a, "listenLater.autoRemove", true)
	pct := readSetting(a, "listenLater.finishedPercent", 90.0)
	if pct <= 0 || pct > 100 {
		pct = 90
	}
	var listed bool
	var dur int64
	err := a.tx.QueryRowContext(a.ctx, `SELECT NOT l.deleted, s.duration_ms FROM listen_later l JOIN songs s ON s.id = l.song_id
		WHERE l.user_id = ? AND l.song_id = ?`, a.sess.UserID, o.Song).Scan(&listed, &dur)
	if errors.Is(err, sql.ErrNoRows) || err == nil && (!on || !listed || dur <= 0 || float64(o.ToMS) < pct/100*float64(dur)) {
		return nil
	}
	if err != nil {
		return err
	}
	// As of the listen's end: adding it again later still wins.
	return a.mark("listen_later", o.Song, true, ended)
}

// readSetting is one of the user's synced settings, or def when it is
// missing, null or malformed. def is the apps' default too: make check
// compares the two (docs/plans/034_settings_text_defaults_reset.md).
func readSetting[T any](a *applier, name string, def T) T {
	var raw string
	v := def
	if a.tx.QueryRowContext(a.ctx, `SELECT value FROM settings WHERE user_id = ? AND name = ?`, a.sess.UserID, name).Scan(&raw) != nil ||
		json.Unmarshal([]byte(raw), &v) != nil {
		return def
	}
	return v
}

// label keeps a client-supplied tag to one short line.
func label(s string, n int) string {
	s = library.OneLine(s)
	if r := []rune(s); len(r) > n {
		s = string(r[:n])
	}
	return s
}

const (
	maxOpsPerPush  = 500
	maxSongsPerOp  = 20000 // a queue of the whole library, with room to grow
	songsPerLookup = 500   // well under SQLite's limit on query parameters
)

// songLookups splits the distinct songs of ids into lookups of
// songsPerLookup, or rejects an op naming too many.
func songLookups(ids []int64) (iter.Seq[[]int64], error) {
	if len(ids) > maxSongsPerOp {
		return nil, rejected("more than %d songs in one operation", maxSongsPerOp)
	}
	return slices.Chunk(slices.Compact(slices.Sorted(slices.Values(ids))), songsPerLookup), nil
}

// inList is an SQL "(?, …)" list for vals, and vals as its arguments.
func inList[T any](vals []T) (string, []any) {
	args := make([]any, len(vals))
	for i, v := range vals {
		args[i] = v
	}
	return "(?" + strings.Repeat(", ?", len(vals)-1) + ")", args
}

func (a *applier) songsExist(ids []int64) error {
	lookups, err := songLookups(ids)
	if err != nil {
		return err
	}
	for chunk := range lookups {
		in, args := inList(chunk)
		var n int
		if err := a.tx.QueryRowContext(a.ctx, `SELECT count(*) FROM songs WHERE id IN `+in, args...).Scan(&n); err != nil {
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
	songs, _ := json.Marshal(q.Songs)
	usedAt := ""
	if touch {
		usedAt = a.now
	}
	return a.write(`UPDATE queues SET version = ?, name = ?, songs = ?, current_song = ?, position_ms = ?,
		current_at = ?, shuffle = ?, repeat = ?, used_at = CASE WHEN ? = '' THEN used_at ELSE ? END
		WHERE id = ?`, q.Name, string(songs), q.CurrentSong, q.PositionMS, q.CurrentAt, q.Shuffle, q.Repeat,
		usedAt, usedAt, q.ID)
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
	// Past 20 queues the least recently used one goes, as in Musicolet.
	for {
		var n int
		if err := a.tx.QueryRowContext(a.ctx, `SELECT count(*) FROM queues WHERE user_id = ? AND deleted = 0`, a.sess.UserID).Scan(&n); err != nil {
			return err
		}
		if n < maxQueues {
			break
		}
		if err := a.write(`UPDATE queues SET version = ?, deleted = 1 WHERE id = (
			SELECT id FROM queues WHERE user_id = ? AND deleted = 0 ORDER BY used_at, created_at LIMIT 1)`, a.sess.UserID); err != nil {
			return err
		}
	}
	current := o.Song
	if current == 0 && len(songs) > 0 {
		current = songs[0]
	}
	data, _ := json.Marshal(songs)
	return a.write(`INSERT INTO queues (version, id, user_id, name, songs, current_song, position_ms, current_at,
		created_at, used_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		o.Queue, a.sess.UserID, name, string(data), current, max(o.PositionMS, 0), a.at, a.now, a.now)
}

// insertAt is where songs go after the given occurrence of `after` in ids:
// nil or a song not there is the end, 0 the start.
func insertAt(ids []int64, after *int64, occurrence int) int {
	if after == nil {
		return len(ids)
	}
	if *after == 0 {
		return 0
	}
	if i := indexOf(ids, *after, occurrence); i >= 0 {
		return i + 1
	}
	return len(ids)
}

// insertAfter puts songs after `after` (see insertAt), moving any that are
// already in the list rather than duplicating them.
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
	return slices.Insert(rest, insertAt(rest, after, 0), songs...)
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
		return a.write(`UPDATE queues SET version = ?, deleted = 1 WHERE id = ?`, q.ID)
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

// mark adds a song to one of the user's special lists (favorites,
// listen_later) or removes it, as of at. The later change wins, so an
// offline device's older change never undoes a newer one.
func (a *applier) mark(table string, song int64, removed bool, at string) error {
	if err := a.songsExist([]int64{song}); err != nil {
		return err
	}
	// table is a constant from the caller, never input.
	if later, err := a.isLater(at, `SELECT at FROM `+table+` WHERE user_id = ? AND song_id = ?`, a.sess.UserID, song); !later {
		return err
	}
	return a.write(`INSERT INTO `+table+` (version, user_id, song_id, at, deleted) VALUES (?, ?, ?, ?, ?)
		ON CONFLICT (user_id, song_id) DO UPDATE SET at = excluded.at, deleted = excluded.deleted, version = excluded.version`,
		a.sess.UserID, song, at, removed)
}

// resume records where the user stopped in a long file, or forgets it once
// the file is finished. Like favorites, the later change wins.
func (a *applier) resume(o op) error {
	if err := a.songsExist([]int64{o.Song}); err != nil {
		return err
	}
	if later, err := a.isLater(a.at, `SELECT at FROM resume_points WHERE user_id = ? AND song_id = ?`, a.sess.UserID, o.Song); !later {
		return err
	}
	return a.write(`INSERT INTO resume_points (version, user_id, song_id, position_ms, at, deleted)
		VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT (user_id, song_id) DO UPDATE SET position_ms = excluded.position_ms,
		at = excluded.at, deleted = excluded.deleted, version = excluded.version`,
		a.sess.UserID, o.Song, max(o.PositionMS, 0), a.at, o.Type == "resume.unset")
}

// Bookmarks are the user's own notes; these limits only keep one user from
// filling the database.
const (
	maxBookmarks    = 10000
	maxBookmarkNote = 1000 // characters
)

// bookmark makes or changes one bookmark, or deletes it. Each merges on its
// own: the later change wins, but a delete wins over any edit, as for
// queues, so a bookmark deleted on one device stays deleted however late
// an offline edit arrives (docs/plans/033_bookmarks.md).
func (a *applier) bookmark(o op) error {
	if o.Bookmark == "" || len(o.Bookmark) > 64 {
		return rejected("bookmark id must be 1–64 characters")
	}
	var at string
	var deleted bool
	err := a.tx.QueryRowContext(a.ctx, `SELECT at, deleted FROM bookmarks WHERE user_id = ? AND id = ?`,
		a.sess.UserID, o.Bookmark).Scan(&at, &deleted)
	exists := err == nil
	if err != nil && !errors.Is(err, sql.ErrNoRows) {
		return err
	}
	if deleted {
		return nil
	}
	if o.Type == "bookmark.unset" {
		if !exists {
			return nil // never seen: nothing to delete
		}
		return a.write(`UPDATE bookmarks SET version = ?, deleted = 1, note = '', at = ? WHERE user_id = ? AND id = ?`,
			max(a.at, at), a.sess.UserID, o.Bookmark)
	}
	if exists && a.at < at {
		return nil
	}
	if utf8.RuneCountInString(o.Note) > maxBookmarkNote {
		return rejected("a bookmark's note is at most %d characters", maxBookmarkNote)
	}
	if err := a.songsExist([]int64{o.Song}); err != nil {
		return err
	}
	if !exists {
		var n int
		if err := a.tx.QueryRowContext(a.ctx, `SELECT count(*) FROM bookmarks WHERE user_id = ? AND NOT deleted`,
			a.sess.UserID).Scan(&n); err != nil {
			return err
		}
		if n >= maxBookmarks {
			return rejected("more than %d bookmarks", maxBookmarks)
		}
	}
	return a.write(`INSERT INTO bookmarks (version, user_id, id, song_id, position_ms, note, at)
		VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT (user_id, id) DO UPDATE SET song_id = excluded.song_id,
		position_ms = excluded.position_ms, note = excluded.note, at = excluded.at, version = excluded.version`,
		a.sess.UserID, o.Bookmark, o.Song, max(o.PositionMS, 0), o.Note, a.at)
}

// Settings are the apps' business; these limits only keep one user from
// filling the database. The count leaves out settings set back to null and
// is sized for a speed per song (`speed.<id>`, plan 016) besides the few
// real settings.
const (
	maxSettings     = 1000
	maxSettingBytes = 4 << 10
)

// setting stores one app setting for every device of the user. The later
// change wins.
func (a *applier) setting(o op) error {
	if o.Name == "" || len(o.Name) > 64 || strings.ContainsFunc(o.Name, func(r rune) bool {
		return !(r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == '.' || r == '_' || r == '-')
	}) {
		return rejected("setting name must be 1–64 letters, digits, '.', '_' or '-'")
	}
	if len(o.Value) == 0 || len(o.Value) > maxSettingBytes {
		return rejected("setting value must be JSON of at most %d bytes", maxSettingBytes)
	}
	if later, err := a.isLater(a.at, `SELECT at FROM settings WHERE user_id = ? AND name = ?`, a.sess.UserID, o.Name); !later {
		return err
	}
	var n int
	if err := a.tx.QueryRowContext(a.ctx, `SELECT count(*) FROM settings WHERE user_id = ? AND name != ? AND value != 'null'`,
		a.sess.UserID, o.Name).Scan(&n); err != nil {
		return err
	}
	if n >= maxSettings {
		return rejected("more than %d settings", maxSettings)
	}
	return a.write(`INSERT INTO settings (version, user_id, name, value, at) VALUES (?, ?, ?, ?, ?)
		ON CONFLICT (user_id, name) DO UPDATE SET value = excluded.value, at = excluded.at, version = excluded.version`,
		a.sess.UserID, o.Name, string(o.Value), a.at)
}

// isLater reports whether a change made at `at` is at least as new as the
// row q finds (an "at" column), or there is none: the later change wins.
func (a *applier) isLater(at, q string, args ...any) (bool, error) {
	var old string
	err := a.tx.QueryRowContext(a.ctx, q, args...).Scan(&old)
	if errors.Is(err, sql.ErrNoRows) {
		return true, nil
	}
	return err == nil && at >= old, err
}

func (a *applier) playbackState(o op) error {
	if later, err := a.isLater(a.at, `SELECT at FROM now_playing WHERE user_id = ?`, a.sess.UserID); !later {
		return err
	}
	var device string
	if err := a.tx.QueryRowContext(a.ctx, `SELECT name FROM devices WHERE id = ?`, a.sess.DeviceID).Scan(&device); err != nil {
		return err
	}
	return a.write(`INSERT INTO now_playing (version, user_id, device_id, device_name, queue_id, song_id, position_ms,
		playing, at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
		ON CONFLICT (user_id) DO UPDATE SET device_id = excluded.device_id, device_name = excluded.device_name,
		queue_id = excluded.queue_id, song_id = excluded.song_id, position_ms = excluded.position_ms,
		playing = excluded.playing, at = excluded.at, version = excluded.version`,
		a.sess.UserID, a.sess.DeviceID, device, o.Queue, o.Song, max(o.PositionMS, 0), o.Playing, a.at)
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
	col, key := "client_ref", any(nil)
	if r, ok := strings.CutPrefix(ref, "ref:"); ok {
		key = r
	} else if id, err := strconv.ParseInt(ref, 10, 64); err == nil {
		col, key = "id", id
	} else {
		return nil, rejected("bad playlist reference %q", ref)
	}
	p := &playlistRow{}
	var deleted bool
	err := a.tx.QueryRowContext(a.ctx, `SELECT id, path, name, owner_id, deleted FROM playlists WHERE `+col+` = ?`, key).
		Scan(&p.ID, &p.Path, &p.Name, &p.Owner, &deleted)
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
	err = library.RefreshPlaylist(a.ctx, a.tx, a.s.Root, p.Path, p.ID, p.Owner)
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

func (a *applier) songEntries(ids []int64) ([]library.WriteEntry, error) {
	// The songs are fetched in the same chunks songsExist uses, not one query
	// each: a replaced playlist may name 20,000 songs.
	lookups, err := songLookups(ids)
	if err != nil {
		return nil, err
	}
	entries := map[int64]library.WriteEntry{}
	for chunk := range lookups {
		in, args := inList(chunk)
		rows, err := a.tx.QueryContext(a.ctx, `SELECT id, kind, path, title, artist, duration_ms FROM songs WHERE id IN `+in, args...)
		if err != nil {
			return nil, err
		}
		for rows.Next() {
			var id, d int64
			var kind, p, t, ar string
			if err := rows.Scan(&id, &kind, &p, &t, &ar, &d); err != nil {
				rows.Close()
				return nil, err
			}
			// A playlist is a file in Music/ and its paths are relative to it; an
			// episode is under another root the file cannot point at (plan 031).
			if kind != library.KindMusic {
				rows.Close()
				return nil, rejected("only music can go in a playlist")
			}
			if ar != "" {
				t = ar + " - " + t
			}
			entries[id] = library.WriteEntry{SongPath: p, Title: t, DurationS: int(d / 1000)}
		}
		rows.Close()
		if err := rows.Err(); err != nil {
			return nil, err
		}
	}
	out := make([]library.WriteEntry, len(ids))
	for i, id := range ids {
		e, ok := entries[id]
		if !ok {
			return nil, rejected("unknown song id")
		}
		out[i] = e
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

// savePlaylist writes p's file at rel and re-indexes it, returning its ID.
func (a *applier) savePlaylist(p *playlistRow, rel string) (int64, error) {
	v, err := store.NextLibraryVersion(a.ctx, a.tx)
	if err != nil {
		return 0, err
	}
	return library.SavePlaylist(a.ctx, a.tx, a.s.Root, a.s.DataDir, rel, p.ID, p.Owner, p.Name, p.entries, v)
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
	id, err := a.savePlaylist(p, rel)
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
		p.entries = slices.Insert(p.entries, insertAt(p.songIDs, o.After, o.AfterOccurrence), entries...)
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
		p.entries = slices.Insert(p.entries, insertAt(ids, o.After, o.AfterOccurrence), e)
	case "playlist.replace":
		if p.entries, err = a.songEntries(o.Songs); err != nil {
			return err
		}
	}
	_, err = a.savePlaylist(p, rel)
	return err
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
	// Each list is what changed since the cursor. A full pull (since 0) leaves
	// out tombstones: the client has nothing to delete.
	changed := []any{userID, since, since}
	queues, err := store.Rows(ctx, tx, `SELECT id, deleted, name, songs, current_song, position_ms, shuffle, repeat, used_at
		FROM queues WHERE user_id = ? AND version > ? AND NOT (deleted AND ? = 0) ORDER BY used_at DESC`, changed,
		func(r *sql.Rows) (q queueJSON, err error) {
			var songs string
			err = r.Scan(&q.ID, &q.Deleted, &q.Name, &songs, &q.CurrentSong, &q.PositionMS, &q.Shuffle, &q.Repeat, &q.UsedAt)
			if q.Deleted {
				q = queueJSON{ID: q.ID, Deleted: true}
			} else {
				q.Songs = json.RawMessage(songs)
			}
			return q, err
		})
	if err != nil {
		return nil, err
	}

	type resumeJSON struct {
		Song       int64  `json:"song"`
		Deleted    bool   `json:"deleted,omitempty"`
		PositionMS int64  `json:"positionMs,omitempty"`
		At         string `json:"at,omitempty"`
	}
	resume, err := store.Rows(ctx, tx, `SELECT song_id, deleted, position_ms, at FROM resume_points
		WHERE user_id = ? AND version > ? AND NOT (deleted AND ? = 0)`, changed, func(r *sql.Rows) (p resumeJSON, err error) {
		err = r.Scan(&p.Song, &p.Deleted, &p.PositionMS, &p.At)
		if p.Deleted {
			p = resumeJSON{Song: p.Song, Deleted: true}
		}
		return p, err
	})
	if err != nil {
		return nil, err
	}

	pairs, err := store.Rows(ctx, tx, `SELECT name, value FROM settings WHERE user_id = ? AND version > ?`, []any{userID, since},
		func(r *sql.Rows) (nv [2]string, err error) {
			err = r.Scan(&nv[0], &nv[1])
			return nv, err
		})
	if err != nil {
		return nil, err
	}
	settings := map[string]json.RawMessage{}
	for _, nv := range pairs {
		settings[nv[0]] = json.RawMessage(nv[1])
	}

	type bookmarkJSON struct {
		ID         string `json:"id"`
		Deleted    bool   `json:"deleted,omitempty"`
		Song       int64  `json:"song,omitempty"`
		PositionMS int64  `json:"positionMs,omitempty"`
		Note       string `json:"note,omitempty"`
		At         string `json:"at,omitempty"`
	}
	bookmarks, err := store.Rows(ctx, tx, `SELECT id, deleted, song_id, position_ms, note, at FROM bookmarks
		WHERE user_id = ? AND version > ? AND NOT (deleted AND ? = 0)`, changed, func(r *sql.Rows) (b bookmarkJSON, err error) {
		err = r.Scan(&b.ID, &b.Deleted, &b.Song, &b.PositionMS, &b.Note, &b.At)
		if b.Deleted {
			b = bookmarkJSON{ID: b.ID, Deleted: true}
		}
		return b, err
	})
	if err != nil {
		return nil, err
	}

	np, err := nowPlaying(ctx, tx, userID, since)
	if err != nil {
		return nil, err
	}
	out := map[string]any{"version": version, "queues": queues, "resume": resume, "bookmarks": bookmarks,
		"settings": settings, "nowPlaying": np}
	// The special lists. table is a constant, never input.
	for key, table := range map[string]string{"favorites": "favorites", "listenLater": "listen_later", "played": "played"} {
		if out[key], err = store.Rows(ctx, tx, `SELECT song_id, deleted, at FROM `+table+` WHERE user_id = ? AND version > ?
			AND NOT (deleted AND ? = 0)`, changed, func(r *sql.Rows) (m markJSON, err error) {
			err = r.Scan(&m.Song, &m.Deleted, &m.At)
			if m.Deleted {
				m.At = ""
			}
			return m, err
		}); err != nil {
			return nil, err
		}
	}
	return out, nil
}

type markJSON struct {
	Song    int64  `json:"song"`
	Deleted bool   `json:"deleted,omitempty"`
	At      string `json:"at,omitempty"` // when it was added: the lists show newest first
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

// countedPercent is how much of a song must be heard for a listen to add to
// its play count (the owner's rule). Shorter listens are still logged.
const countedPercent = 50

// playCounts returns the caller's play count and last counted play per song,
// for sorting by most / recently played on the client. Music only: an
// episode or a book has played marks instead (plan 031).
func (s *Server) playCounts(w http.ResponseWriter, r *http.Request, sess session) {
	type pc struct {
		Song       int64  `json:"song"`
		Count      int    `json:"count"`
		LastPlayed string `json:"lastPlayedAt"`
	}
	out, err := store.Rows(r.Context(), s.DB, `SELECT p.song_id, count(*), max(p.at)
		FROM plays p JOIN songs s ON s.id = p.song_id
		WHERE p.user_id = ? AND s.kind = 'music' AND p.ms_played > 0 AND p.ms_played * 100 >= s.duration_ms * ?
		GROUP BY p.song_id`, []any{sess.UserID, countedPercent}, func(rows *sql.Rows) (p pc, err error) {
		err = rows.Scan(&p.Song, &p.Count, &p.LastPlayed)
		return p, err
	})
	if err != nil {
		s.fail(w, r, err)
		return
	}
	writeJSON(w, r, map[string]any{"plays": out})
}
