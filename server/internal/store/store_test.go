package store

import (
	"database/sql"
	"path/filepath"
	"testing"
)

// The songs rebuild (0009) runs on the owner's live database: every ID, and
// everything pointing at one, must come through, and no ID may be reused.
func TestSongsRebuildKeepsIDsAndReferences(t *testing.T) {
	db, err := sql.Open("sqlite", "file:"+filepath.Join(t.TempDir(), "d.db")+"?_pragma=foreign_keys(1)")
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	if err := migrate(db, 8); err != nil {
		t.Fatal(err)
	}
	exec := func(q string, args ...any) {
		t.Helper()
		if _, err := db.Exec(q, args...); err != nil {
			t.Fatalf("%s: %v", q, err)
		}
	}
	exec(`INSERT INTO users (name, password_hash, created_at) VALUES ('a', 'x', 't')`)
	for _, p := range []string{"a.mp3", "b.mp3", "c.mp3"} {
		exec(`INSERT INTO songs (path, size, mtime_ns, quick_hash, title, art, added_at, updated_at, version)
			VALUES (?, 1, 1, x'00', 't', 'k', 't', 't', 1)`, p)
	}
	exec(`DELETE FROM songs WHERE path = 'c.mp3'`) // id 3 must still never come back
	exec(`INSERT INTO favorites (user_id, song_id, at, version) VALUES (1, 2, 't', 1)`)

	if err := migrate(db, 9); err != nil {
		t.Fatal(err)
	}
	var id int64
	var kind, art string
	if err := db.QueryRow(`SELECT id, kind, art FROM songs WHERE path = 'b.mp3'`).Scan(&id, &kind, &art); err != nil {
		t.Fatal(err)
	}
	if id != 2 || kind != "music" || art != "k" {
		t.Errorf("b.mp3 = id %d, kind %q, art %q; want 2, music, k", id, kind, art)
	}
	exec(`INSERT INTO songs (kind, path, size, mtime_ns, quick_hash, title, added_at, updated_at, version)
		VALUES ('podcast', 'a.mp3', 1, 1, x'00', 't', 't', 't', 2)`)
	if err := db.QueryRow(`SELECT max(id) FROM songs`).Scan(&id); err != nil {
		t.Fatal(err)
	}
	if id != 4 {
		t.Errorf("the next song got id %d, want 4: AUTOINCREMENT's counter was lost", id)
	}
	// References still point at songs, and are still enforced.
	if _, err := db.Exec(`INSERT INTO favorites (user_id, song_id, at, version) VALUES (1, 99, 't', 1)`); err == nil {
		t.Error("a favorite of a song that does not exist was accepted: foreign keys are off")
	}
}
