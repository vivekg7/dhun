package library

import (
	"context"
	"database/sql"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"testing"
	"time"

	"go.senan.xyz/taglib"

	"github.com/vivekg7/dhun/server/internal/store"
)

// setup returns a scanner over an empty media root and a helper that copies a
// testdata fixture to a path under it.
func setup(t *testing.T) (*Scanner, func(fixture, rel string)) {
	t.Helper()
	root := t.TempDir()
	db, err := store.Open(filepath.Join(t.TempDir(), "dhun.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { db.Close() })
	s := &Scanner{DB: db, Root: root, Log: slog.New(slog.NewTextHandler(io.Discard, nil))}
	put := func(fixture, rel string) {
		t.Helper()
		data, err := os.ReadFile(filepath.Join("testdata", fixture))
		if err != nil {
			t.Fatal(err)
		}
		writeFile(t, filepath.Join(root, rel), data)
	}
	return s, put
}

func writeFile(t *testing.T, abs string, data []byte) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(abs), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(abs, data, 0o644); err != nil {
		t.Fatal(err)
	}
}

func scan(t *testing.T, s *Scanner) Stats {
	t.Helper()
	st, err := s.Scan(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	return st
}

func songID(t *testing.T, db *sql.DB, rel string) int64 {
	t.Helper()
	var id int64
	if err := db.QueryRow(`SELECT id FROM songs WHERE path = ?`, rel).Scan(&id); err != nil {
		t.Fatalf("song %s: %v", rel, err)
	}
	return id
}

func TestInitialScanReadsTagsAndSkipsNonCollectionFolders(t *testing.T) {
	s, put := setup(t)
	put("a.mp3", "Library/Adele/25 (2015)/1-01 - Hello.mp3")
	put("b.m4a", "Library/A.R. Rahman/Jab Tak Hai Jaan (2012)/1-09 - Saans.m4a")
	put("c.opus", "Collection/Motivation/1-02 - Do Not Go Gentle.opus")
	put("a.mp3", "_inbox/jiosaavn-2026-10-05/x.mp3")             // curation inbox
	put("a.mp3", "_trash/dedupe-2026-10-06/y.mp3")               // curation trash
	put("a.mp3", "Library/Adele/@eaDir/Hello.mp3/z.mp3")         // Synology thumbnails
	put("a.mp3", "Library/Adele/.hidden/w.mp3")                  // hidden folder
	put("a.mp3", "#snapshot/GMT+05-2026.10.06/Library/v.mp3")    // Synology snapshots
	put("a.mp3", "#recycle/Library/u.mp3")                       // Synology recycle bin
	put("a.mp3", "Library/Various/#1. Deer Hunter (1991)/t.mp3") // an album, not a system folder

	st := scan(t, s)
	if st.Added != 4 {
		t.Fatalf("added %d songs, want 4 (%s)", st.Added, st)
	}

	var title, artists, genres string
	var year, track int
	var dur int64
	err := s.DB.QueryRow(`SELECT title, artists, genres, year, track, duration_ms FROM songs WHERE path LIKE '%Saans.m4a'`).
		Scan(&title, &artists, &genres, &year, &track, &dur)
	if err != nil {
		t.Fatal(err)
	}
	if title != "Saans" || year != 2012 || track != 9 {
		t.Errorf("tags: title=%q year=%d track=%d", title, year, track)
	}
	if artists != `["A.R. Rahman","Shreya Ghoshal","Mohit Chauhan"]` || genres != `["Bollywood","Romance"]` {
		t.Errorf("split: artists=%s genres=%s", artists, genres)
	}
	if dur < 900 || dur > 1100 {
		t.Errorf("duration %dms, want ~1000", dur)
	}

	// No TITLE tag: the file name without its "1-02 - " track prefix.
	if err := s.DB.QueryRow(`SELECT title FROM songs WHERE path LIKE '%.opus'`).Scan(&title); err != nil {
		t.Fatal(err)
	}
	if title != "Do Not Go Gentle" {
		t.Errorf("fallback title %q", title)
	}

	if st := scan(t, s); st.Unchanged != 4 || st.Added+st.Updated+st.Moved+st.Missing != 0 {
		t.Errorf("rescan of an unchanged tree: %s", st)
	}
}

// A move must keep the song's ID, or every play count and favorite pointing
// at it is orphaned (plan 005).
func TestMoveKeepsSongID(t *testing.T) {
	s, put := setup(t)
	put("a.mp3", "Library/Shreya Ghoshal/Singles/Hello.mp3")
	scan(t, s)
	id := songID(t, s.DB, "Library/Shreya Ghoshal/Singles/Hello.mp3")

	old := filepath.Join(s.Root, "Library/Shreya Ghoshal/Singles/Hello.mp3")
	moved := filepath.Join(s.Root, "Library/Adele/25 (2015)/1-01 - Hello.mp3")
	os.MkdirAll(filepath.Dir(moved), 0o755)
	if err := os.Rename(old, moved); err != nil {
		t.Fatal(err)
	}

	st := scan(t, s)
	if st.Moved != 1 || st.Added != 0 || st.Missing != 0 {
		t.Fatalf("after move: %s", st)
	}
	if got := songID(t, s.DB, "Library/Adele/25 (2015)/1-01 - Hello.mp3"); got != id {
		t.Errorf("moved song has id %d, want %d", got, id)
	}
}

// A vanished file keeps its row (and so its user data); when it comes back,
// even somewhere else, it is the same song again.
func TestMissingSongComesBackWithItsID(t *testing.T) {
	s, put := setup(t)
	put("b.m4a", "Library/A/Saans.m4a")
	put("a.mp3", "Library/B/Hello.mp3") // so the library is not left empty
	scan(t, s)
	id := songID(t, s.DB, "Library/A/Saans.m4a")

	data, _ := os.ReadFile(filepath.Join(s.Root, "Library/A/Saans.m4a"))
	os.Remove(filepath.Join(s.Root, "Library/A/Saans.m4a"))
	if st := scan(t, s); st.Missing != 1 {
		t.Fatalf("after delete: %s", st)
	}

	writeFile(t, filepath.Join(s.Root, "Library/B/Saans.m4a"), data)
	if st := scan(t, s); st.Moved != 1 || st.Added != 0 {
		t.Fatalf("after reappearing: %s", st)
	}
	var missing sql.NullString
	s.DB.QueryRow(`SELECT missing_since FROM songs WHERE id = ?`, id).Scan(&missing)
	if got := songID(t, s.DB, "Library/B/Saans.m4a"); got != id || missing.Valid {
		t.Errorf("reappeared as id %d (missing=%v), want %d and not missing", got, missing.Valid, id)
	}
}

// Lyrics and covers are fetched into the library later than the audio. The
// audio file does not change, so the flags must be re-checked on every scan.
func TestLaterLrcAndCoverAreNoticed(t *testing.T) {
	s, put := setup(t)
	put("a.mp3", "Library/Adele/25/1-01 - Hello.mp3")
	scan(t, s)

	writeFile(t, filepath.Join(s.Root, "Library/Adele/25/1-01 - Hello.lrc"), []byte("[00:00.10]Hello"))
	writeFile(t, filepath.Join(s.Root, "Library/Adele/25/Cover.JPG"), []byte("jpeg"))
	if st := scan(t, s); st.Updated != 1 {
		t.Fatalf("after adding .lrc and cover: %s", st)
	}
	var lrc bool
	var art string
	s.DB.QueryRow(`SELECT lrc, folder_art FROM songs`).Scan(&lrc, &art)
	if !lrc || art != "Cover.JPG" {
		t.Errorf("lrc=%v folder_art=%q", lrc, art)
	}
}

func TestChangedFileIsReread(t *testing.T) {
	s, put := setup(t)
	put("a.mp3", "Library/x.mp3")
	scan(t, s)
	id := songID(t, s.DB, "Library/x.mp3")

	// Re-tagged outside Dhun: same path, new content.
	if err := taglib.WriteTags(filepath.Join(s.Root, "Library/x.mp3"),
		map[string][]string{taglib.Title: {"Hello (Remastered)"}}, 0); err != nil {
		t.Fatal(err)
	}
	future := time.Now().Add(time.Hour)
	os.Chtimes(filepath.Join(s.Root, "Library/x.mp3"), future, future)
	if st := scan(t, s); st.Updated != 1 {
		t.Fatalf("after replacing the file: %s", st)
	}
	var title string
	s.DB.QueryRow(`SELECT title FROM songs WHERE id = ?`, id).Scan(&title)
	if title != "Hello (Remastered)" {
		t.Errorf("title %q after re-read, want the new tag", title)
	}
}

func TestPlaylistsResolveOwnershipAndTombstones(t *testing.T) {
	s, put := setup(t)
	put("a.mp3", "Library/Adele/25 (2015)/1-01 - Hello.mp3")
	put("b.m4a", "Collection/Motivation/Saans.m4a")
	if _, err := s.DB.Exec(`INSERT INTO users (name, password_hash, created_at) VALUES ('vivek', 'x', 'now')`); err != nil {
		t.Fatal(err)
	}
	shared := "\uFEFF#EXTM3U\n#PLAYLIST:Collection - Motivation\n# a comment\n" +
		"#EXTINF:1,Adele - Hello\n../Library/Adele/25 (2015)/1-01 - Hello.mp3\n" +
		"#EXTINF:1,Saans\n../Collection/Motivation/Saans.m4a\n" +
		"../Library/Gone/missing.mp3\n"
	writeFile(t, filepath.Join(s.Root, "Playlists/Collection - Motivation.m3u8"), []byte(shared))
	writeFile(t, filepath.Join(s.Root, "Playlists/vivek/Drive.m3u8"),
		[]byte("#EXTM3U\n../../Library/Adele/25 (2015)/1-01 - Hello.mp3\n"))

	if st := scan(t, s); st.Playlists != 2 {
		t.Fatalf("indexed %d playlists, want 2", st.Playlists)
	}

	var name string
	var owner sql.NullInt64
	s.DB.QueryRow(`SELECT name, owner_id FROM playlists WHERE path = 'Playlists/Collection - Motivation.m3u8'`).Scan(&name, &owner)
	if name != "Collection - Motivation" || owner.Valid {
		t.Errorf("shared playlist: name=%q owner=%v", name, owner)
	}
	s.DB.QueryRow(`SELECT owner_id FROM playlists WHERE path = 'Playlists/vivek/Drive.m3u8'`).Scan(&owner)
	if !owner.Valid {
		t.Error("Playlists/vivek/ is not owned by user vivek")
	}

	var resolved, unresolved int
	s.DB.QueryRow(`SELECT count(song_id), count(*) - count(song_id) FROM playlist_items i
		JOIN playlists p ON p.id = i.playlist_id WHERE p.path LIKE '%Motivation%'`).Scan(&resolved, &unresolved)
	if resolved != 2 || unresolved != 1 {
		t.Errorf("resolved=%d unresolved=%d, want 2 and 1 (an unresolved entry is kept, never dropped)", resolved, unresolved)
	}

	os.Remove(filepath.Join(s.Root, "Playlists/vivek/Drive.m3u8"))
	scan(t, s)
	var deleted bool
	s.DB.QueryRow(`SELECT deleted FROM playlists WHERE path = 'Playlists/vivek/Drive.m3u8'`).Scan(&deleted)
	if !deleted {
		t.Error("a removed playlist file must leave a tombstone so clients drop it")
	}
}

// A mistyped or unmounted Music path is an empty folder to the container.
// That must not mark the collection missing or tombstone the playlists.
func TestEmptyMountChangesNothing(t *testing.T) {
	s, put := setup(t)
	put("a.mp3", "Library/A/Hello.mp3")
	writeFile(t, filepath.Join(s.Root, "Playlists", "Mix.m3u8"), []byte("../Library/A/Hello.mp3\n"))
	scan(t, s)

	os.RemoveAll(filepath.Join(s.Root, "Library"))
	if _, err := s.Scan(t.Context()); err == nil {
		t.Error("scanned an empty media folder without complaint")
	}
	put("a.mp3", "Library/A/Hello.mp3")
	os.RemoveAll(filepath.Join(s.Root, "Playlists"))
	if _, err := s.Scan(t.Context()); err == nil {
		t.Error("scanned without Playlists/ without complaint")
	}
	var missing, deleted int
	s.DB.QueryRow(`SELECT count(*) FROM songs WHERE missing_since IS NOT NULL`).Scan(&missing)
	s.DB.QueryRow(`SELECT count(*) FROM playlists WHERE deleted = 1`).Scan(&deleted)
	if missing != 0 || deleted != 0 {
		t.Errorf("%d songs marked missing, %d playlists deleted; want none", missing, deleted)
	}
}

// Files copied from a Mac are often named in decomposed Unicode (é as e plus
// a combining accent) while playlists use the composed form. On the owner's
// NAS this left 118 of 911 entries unresolved before paths were compared in
// NFC.
func TestPlaylistMatchesDecomposedFileNames(t *testing.T) {
	s, put := setup(t)
	put("a.mp3", "Library/Beyonce\u0301/Halo.mp3")                                                             // NFD on disk
	writeFile(t, filepath.Join(s.Root, "Playlists", "Mix.m3u8"), []byte("../Library/Beyonc\u00e9/Halo.mp3\n")) // NFC
	scan(t, s)
	var resolved int
	s.DB.QueryRow(`SELECT count(*) FROM playlist_items WHERE song_id IS NOT NULL`).Scan(&resolved)
	if resolved != 1 {
		t.Error("a composed playlist entry did not match the decomposed file name")
	}
}

func artOf(t *testing.T, db *sql.DB, rel string) string {
	t.Helper()
	var art string
	if err := db.QueryRow(`SELECT art FROM songs WHERE path = ?`, rel).Scan(&art); err != nil {
		t.Fatalf("song %s: %v", rel, err)
	}
	return art
}

// Songs showing one cover share one key, so the apps fetch and keep it once;
// a changed cover gets a new key, or the apps would keep the old one.
func TestArtKeysFollowTheCover(t *testing.T) {
	s, put := setup(t)
	put("a.mp3", "Library/Album/1.mp3")
	put("a.mp3", "Library/Album/2.mp3")
	put("b.m4a", "Library/Other/3.m4a")
	scan(t, s)
	if a := artOf(t, s.DB, "Library/Album/1.mp3"); a != "" {
		t.Fatalf("art %q with no cover", a)
	}

	cover := filepath.Join(s.Root, "Library/Album/cover.jpg")
	writeFile(t, cover, []byte("jpeg"))
	scan(t, s)
	a1, a2 := artOf(t, s.DB, "Library/Album/1.mp3"), artOf(t, s.DB, "Library/Album/2.mp3")
	if a1 == "" || a1 != a2 {
		t.Fatalf("one folder cover gave keys %q and %q", a1, a2)
	}

	writeFile(t, cover, []byte("a new jpeg"))
	if st := scan(t, s); st.Updated != 2 {
		t.Fatalf("after replacing the cover: %s", st)
	}
	if a := artOf(t, s.DB, "Library/Album/1.mp3"); a == a1 {
		t.Error("a replaced cover kept its key")
	}

	// Embedded art wins over the folder's, and the same image is one key.
	img := []byte("\xff\xd8\xff\xe0 not really a jpeg")
	for _, rel := range []string{"Library/Album/1.mp3", "Library/Other/3.m4a"} {
		abs := filepath.Join(s.Root, rel)
		if err := taglib.WriteImage(abs, img); err != nil {
			t.Fatal(err)
		}
		future := time.Now().Add(time.Hour)
		os.Chtimes(abs, future, future)
	}
	scan(t, s)
	e1, e3 := artOf(t, s.DB, "Library/Album/1.mp3"), artOf(t, s.DB, "Library/Other/3.m4a")
	if e1 == "" || e1 != e3 || e1 == artOf(t, s.DB, "Library/Album/2.mp3") {
		t.Errorf("embedded keys %q, %q; folder key %q", e1, e3, artOf(t, s.DB, "Library/Album/2.mp3"))
	}
}

// A library scanned before art keys existed gets them on the next scan,
// embedded art included, though no file changed.
func TestArtKeysAreFilledIn(t *testing.T) {
	s, put := setup(t)
	put("a.mp3", "Library/1.mp3")
	if err := taglib.WriteImage(filepath.Join(s.Root, "Library/1.mp3"), []byte("\xff\xd8\xff\xe0 image")); err != nil {
		t.Fatal(err)
	}
	scan(t, s)
	want := artOf(t, s.DB, "Library/1.mp3")
	s.DB.Exec(`UPDATE songs SET art = ''`)
	if st := scan(t, s); st.Updated != 1 {
		t.Fatalf("filling in the key: %s", st)
	}
	if got := artOf(t, s.DB, "Library/1.mp3"); got == "" || got != want {
		t.Errorf("key %q, want %q", got, want)
	}
	if st := scan(t, s); st.Unchanged != 1 {
		t.Errorf("a song with its key was read again: %s", st)
	}
}
