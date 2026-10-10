package library

import (
	"os"
	"path/filepath"
	"testing"
)

// Chapters live in three places, by container: CHAPTERnnn comments in Ogg,
// CHAP frames in MP3's ID3v2 tag (which taglib does not return), and the
// Chapters element in WebM. Fixtures made by ffmpeg with two chapters.
func TestChaptersFromEachContainer(t *testing.T) {
	for _, f := range []string{"chapters.opus", "chapters.mp3", "chapters-webm.opus"} {
		tg, err := readTags(filepath.Join("testdata", f), f, true)
		if err != nil {
			t.Fatalf("%s: %v", f, err)
		}
		if len(tg.Chapters) != 2 || tg.Chapters[0] != (Chapter{0, "Introduction"}) || tg.Chapters[1].Title != "Second part" || tg.Chapters[1].StartMS < 1000 {
			t.Errorf("%s: chapters %+v", f, tg.Chapters)
		}
		if tg.Date != "2020-08-16" || tg.Notes != "Notes line one." {
			t.Errorf("%s: date %q, notes %q", f, tg.Date, tg.Notes)
		}
	}
}

// YouTube's automatic captions scroll: every cue repeats the line before it,
// and a 10 ms cue repeats it once more. Shown as lyrics, each line must
// appear once, at the time it was first said.
func TestAutoCaptionsKeepEachLineOnce(t *testing.T) {
	vtt := `WEBVTT
Kind: captions
Language: en

00:00:00.000 --> 00:00:02.360 align:start position:0%
 
as<00:00:00.089><c> part</c><00:00:00.390><c> of</c>

00:00:02.360 --> 00:00:02.370 align:start position:0%
as part of
 

00:00:02.370 --> 00:00:04.280 align:start position:0%
as part of
artificial<00:00:03.030><c> general</c>

01:40:02.370 --> 01:40:04.280
Q&amp;A
`
	want := "[00:00.00]as part of\n[00:02.37]artificial general\n[100:02.37]Q&A\n"
	if got := vttToLRC(vtt); got != want {
		t.Errorf("got\n%s\nwant\n%s", got, want)
	}
}

func TestMarkdownTranscriptKeepsSpeakers(t *testing.T) {
	md := "# #400 – Elon Musk\n\n## War and human nature\n\n**Lex Fridman** (00:00:00): The following is a conversation.\n\n**Elon Musk** (01:02:03): Yeah.\n"
	l := transcriptLyrics("x.transcript.md", md)
	if want := "[00:00.00]Lex Fridman: The following is a conversation.\n[62:03.00]Elon Musk: Yeah.\n"; l.Text != want || !l.Synced || l.Source != "transcript" {
		t.Errorf("got %+v, want text %q", l, want)
	}
}

// Podcasts and audiobooks are scanned from their own roots: an episode keeps
// its chapters, transcript and show, and a Podcasts folder that is not
// mounted must not mark every episode missing, nor stop the music scan.
func TestSpokenRootsScanOnTheirOwn(t *testing.T) {
	s, put := setup(t)
	s.Podcasts, s.Audiobooks = t.TempDir(), t.TempDir()
	copyTo := func(fixture, abs string) {
		data, err := os.ReadFile(filepath.Join("testdata", fixture))
		if err != nil {
			t.Fatal(err)
		}
		writeFile(t, abs, data)
	}
	put("a.mp3", "Show/001 - Ep.mp3") // the same path in Music is another file
	copyTo("chapters.opus", filepath.Join(s.Podcasts, "Show/001 - Ep.opus"))
	writeFile(t, filepath.Join(s.Podcasts, "Show/001 - Ep.en.auto.vtt"), []byte("WEBVTT\n"))
	writeFile(t, filepath.Join(s.Podcasts, "Show/001 - Ep.en.vtt"), []byte("WEBVTT\n"))
	copyTo("chapters.mp3", filepath.Join(s.Audiobooks, "Book/Part 1.mp3"))
	if st := scan(t, s); st.Added != 3 {
		t.Fatalf("added %d, want 3 (%s)", st.Added, st)
	}

	var kind, transcript, chapters, group string
	if err := s.DB.QueryRow(`SELECT kind, transcript, chapters, group_title FROM songs WHERE path = 'Show/001 - Ep.opus'`).
		Scan(&kind, &transcript, &chapters, &group); err != nil {
		t.Fatal(err)
	}
	if kind != KindPodcast || transcript != "001 - Ep.en.vtt" || group != "Test Show" ||
		chapters != `[{"startMs":0,"title":"Introduction"},{"startMs":2500,"title":"Second part"}]` {
		t.Errorf("episode: kind %q, transcript %q, show %q, chapters %s", kind, transcript, group, chapters)
	}
	if err := s.DB.QueryRow(`SELECT chapters FROM songs WHERE kind = 'music'`).Scan(&chapters); err != nil || chapters != "[]" {
		t.Errorf("music chapters %q, %v: music is not read for chapters", chapters, err)
	}

	os.RemoveAll(s.Podcasts) // the volume is not mounted
	if st := scan(t, s); st.Missing != 0 {
		t.Errorf("an unmounted Podcasts marked %d files missing", st.Missing)
	}
}
