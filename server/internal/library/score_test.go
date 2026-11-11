package library

import (
	"database/sql"
	"path/filepath"
	"slices"
	"strings"
	"testing"
)

func writeScores(t *testing.T, s *Scanner, stats string) {
	t.Helper()
	writeFile(t, filepath.Join(s.Root, "_meta/index/songs.jsonl"), []byte(strings.Join([]string{
		`{"song_id": "sa", "path": "Library/A/a.mp3"}`,
		`{"song_id": "sb", "path": "Library/B/b.m4a"}`,
	}, "\n")+"\n"))
	writeFile(t, filepath.Join(s.Root, "_meta/index/stats.jsonl"), []byte(stats))
}

func scoreAndVersion(t *testing.T, db *sql.DB, rel string) (score int, version int64) {
	t.Helper()
	if err := db.QueryRow(`SELECT score, version FROM songs WHERE path = ?`, rel).Scan(&score, &version); err != nil {
		t.Fatal(err)
	}
	return
}

// Scores reach the songs by path, and a rescan bumps the version of only
// the songs whose rounded score moved: a monthly refresh must not make
// every app pull the whole library again.
func TestScoresFollowTheIndex(t *testing.T) {
	s, put := setup(t)
	put("a.mp3", "Library/A/a.mp3")
	put("b.m4a", "Library/B/b.m4a")
	scan(t, s) // no score files yet: nothing fails
	if sc, _ := scoreAndVersion(t, s.DB, "Library/A/a.mp3"); sc != 0 {
		t.Fatalf("score with no index = %d, want 0", sc)
	}

	writeScores(t, s, `{"song_id":"sa","score":97.6,"score_sources":3}`+"\n"+`{"song_id":"sb","score":40.2}`+"\n")
	if st := scan(t, s); st.Scored != 2 {
		t.Fatalf("scored = %d, want 2", st.Scored)
	}
	a, va := scoreAndVersion(t, s.DB, "Library/A/a.mp3")
	b, vb := scoreAndVersion(t, s.DB, "Library/B/b.m4a")
	if a != 98 || b != 40 {
		t.Fatalf("scores = %d, %d; want 98, 40", a, b)
	}

	// 40.4 rounds as 40.2 did; a's line is gone, so a has no score now.
	writeScores(t, s, `{"song_id":"sb","score":40.4}`+"\n"+`{"song_id":"sc","score":12}`+"\n")
	if st := scan(t, s); st.Scored != 1 {
		t.Fatalf("scored = %d, want 1", st.Scored)
	}
	if a2, va2 := scoreAndVersion(t, s.DB, "Library/A/a.mp3"); a2 != 0 || va2 == va {
		t.Errorf("a = score %d version %d; want 0 and a new version", a2, va2)
	}
	if _, vb2 := scoreAndVersion(t, s.DB, "Library/B/b.m4a"); vb2 != vb {
		t.Errorf("b's version moved from %d to %d with the same rounded score", vb, vb2)
	}
}

// A file caught halfway through being written keeps every score as it was.
func TestHalfWrittenScoresChangeNothing(t *testing.T) {
	s, put := setup(t)
	put("a.mp3", "Library/A/a.mp3")
	put("b.m4a", "Library/B/b.m4a")
	writeScores(t, s, `{"song_id":"sa","score":60}`+"\n"+`{"song_id":"sb","score":70}`+"\n")
	scan(t, s)
	writeScores(t, s, `{"song_id":"sa","score":10}`+"\n"+`{"song_id":"sb","sc`)
	if st := scan(t, s); st.Scored != 0 {
		t.Errorf("scored = %d from a broken file, want 0", st.Scored)
	}
	if a, _ := scoreAndVersion(t, s.DB, "Library/A/a.mp3"); a != 60 {
		t.Errorf("a = %d, want 60 kept", a)
	}
}

func TestGenresKeepAnAmpersandInsideAWord(t *testing.T) {
	got := splitOn(genreSeparators, []string{"R&B/Soul", "Sufi & Ghazals; Pop"})
	if want := []string{"R&B/Soul", "Sufi", "Ghazals", "Pop"}; !slices.Equal(got, want) {
		t.Errorf("genres = %q, want %q", got, want)
	}
	if got := splitOn(nameSeparators, []string{"Vishal&Shekhar"}); len(got) != 2 {
		t.Errorf("artists = %q: artists still split on any &", got)
	}
}
