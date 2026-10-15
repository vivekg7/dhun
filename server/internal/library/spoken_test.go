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

// The owner's tool writes older episodes' captions into the lyrics tag as
// WebVTT with no blank lines between cues. Sent raw, no app could show it.
func TestCaptionsInTheLyricsTag(t *testing.T) {
	vtt := "WEBVTT\nKind: captions\nLanguage: en\n" +
		"00:00:00.000 --> 00:00:02.360 align:start position:0%\nas<00:00:00.089><c> part</c><00:00:00.390><c> of</c>\n" +
		"00:00:02.360 --> 00:00:02.370 align:start position:0%\nas part of\n" +
		"00:00:02.370 --> 00:00:04.280 align:start position:0%\nas part of\nartificial<00:00:03.030><c> general</c>\n"
	want := "[00:00.00]as part of\n[00:02.37]artificial general\n"
	if l := embeddedLyrics(vtt); l.Text != want || !l.Synced || l.Source != "vtt" {
		t.Errorf("got %+v, want text %q", l, want)
	}
}

// An .lrc beside an episode wins over its other captions and transcript
// (the owner, 2026-10-15), whether named as the audio or with a language
// as subtitle tools write it. A song's .lrc must match its name exactly.
func TestLrcComesFirst(t *testing.T) {
	names := []string{"e.opus", "e.en.vtt", "e.transcript.md", "e.en.auto.lrc", "e.en.lrc"}
	if n := lrcName(names, "e.opus", true); n != "e.en.lrc" {
		t.Errorf("tagged: got %q", n)
	}
	if n := lrcName(append(names, "E.lrc"), "e.opus", true); n != "E.lrc" {
		t.Errorf("exact: got %q", n)
	}
	if n := lrcName(names, "e.opus", false); n != "" {
		t.Errorf("a song took %q", n)
	}

	dir := t.TempDir()
	for name, text := range map[string]string{"e.en.lrc": "[00:01.00]from the lrc\n", "e.en.vtt": "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nfrom the vtt\n"} {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(text), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	f := SongFile{Root: dir, Path: "e.opus", Lrc: true, Spoken: true, Transcript: "e.en.vtt"}
	if l, err := ReadLyrics(f); err != nil || l.Source != "lrc" || l.Text != "[00:01.00]from the lrc\n" {
		t.Errorf("got %+v, %v", l, err)
	}
}

// YouTube's automatic captions converted to an LRC file scroll as the vtt
// does: each line three times, a blank between. An episode keeps each line
// once, at the time it first appears; a song keeps its LRC as written, since
// a chorus may repeat a line straight after itself.
func TestRollingCaptionsInAnLrcKeepEachLineOnce(t *testing.T) {
	dir := t.TempDir()
	lrc := "[re:Lavf62.3.100]\n\n" +
		"[00:00.24]Hello and welcome. In this episode\n" +
		"[00:02.87]Hello and welcome. In this episode\n[00:02.87] \n" +
		"[00:02.88]Hello and welcome. In this episode\n[00:02.88]we read a book.\n" +
		"[00:05.91]we read a book.\n[00:05.91] \n[00:05.92]we read a book.\n" +
		"[00:05.92]Hey\n[00:07.00]Hey\n"
	if err := os.WriteFile(filepath.Join(dir, "e.lrc"), []byte(lrc), 0o644); err != nil {
		t.Fatal(err)
	}
	f := SongFile{Root: dir, Path: "e.mp3", Lrc: true, Spoken: true}
	want := "[re:Lavf62.3.100]\n[00:00.24]Hello and welcome. In this episode\n" +
		"[00:02.88]we read a book.\n[00:05.92]Hey\n"
	if l, err := ReadLyrics(f); err != nil || l.Text != want || !l.Synced {
		t.Errorf("episode: got %+v, %v\nwant\n%s", l, err, want)
	}
	f.Spoken = false
	if l, err := ReadLyrics(f); err != nil || l.Text != lrc {
		t.Errorf("a song's lyrics changed: %+v, %v", l, err)
	}
}

func TestMarkdownTranscriptKeepsSpeakers(t *testing.T) {
	md := "# #400 – Elon Musk\n\n## War and human nature\n\n**Lex Fridman** (00:00:00): The following is a conversation.\n\n**Elon Musk** (01:02:03): Yeah.\n"
	want := "[00:00.00]Lex Fridman: The following is a conversation.\n[62:03.00]Elon Musk: Yeah.\n"
	// The owner's tool writes the same Markdown into the episode's lyrics
	// tag, which is read first: it must come out converted too.
	for _, l := range []Lyrics{transcriptLyrics("x.transcript.md", md), embeddedLyrics(md)} {
		if l.Text != want || !l.Synced || l.Source != "transcript" {
			t.Errorf("got %+v, want text %q", l, want)
		}
	}
	if l := embeddedLyrics("**Chorus**\nLa la la"); l.Source != "embedded" || l.Text != "**Chorus**\nLa la la" {
		t.Errorf("a song's lyrics changed: %+v", l)
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
