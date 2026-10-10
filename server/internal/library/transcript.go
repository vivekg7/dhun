package library

import (
	"fmt"
	"path"
	"regexp"
	"strings"
)

// Transcripts of podcast episodes are served as lyrics, converted on the
// server into the LRC the apps already parse, so no app needs a parser of
// its own for them (docs/plans/031_podcasts_and_audiobooks.md).

// transcriptName picks an audio file's transcript among its folder's file
// names: a .transcript.md first (speakers, punctuation), then subtitles
// written by people (.en.vtt), then automatic ones (.en.auto.vtt).
func transcriptName(names []string, audio string) string {
	base := strings.ToLower(strings.TrimSuffix(audio, path.Ext(audio)))
	best, rank := "", 0
	for _, n := range names {
		l := strings.ToLower(n)
		if !strings.HasPrefix(l, base+".") {
			continue
		}
		r := 0
		switch {
		case l == base+".transcript.md":
			r = 3
		case strings.HasSuffix(l, ".auto.vtt"):
			r = 1
		case strings.HasSuffix(l, ".vtt"):
			r = 2
		}
		if r > rank {
			best, rank = n, r
		}
	}
	return best
}

// transcriptLyrics converts a transcript file's text into lyrics.
func transcriptLyrics(name, text string) Lyrics {
	text = strings.TrimPrefix(text, "\uFEFF")
	if strings.HasSuffix(strings.ToLower(name), ".vtt") {
		return makeLyrics("vtt", vttToLRC(text))
	}
	if lrc := markdownToLRC(text); lrc != "" {
		return makeLyrics("transcript", lrc)
	}
	return makeLyrics("transcript", text) // no timestamps: plain text
}

func lrcTime(ms int64) string {
	return fmt.Sprintf("[%02d:%02d.%02d]", ms/60000, ms/1000%60, ms%1000/10)
}

var vttTag = regexp.MustCompile(`<[^>]*>`)

// vttToLRC keeps each line of speech once, at the time it first appears.
// YouTube's automatic captions scroll: each cue repeats the line before it
// and the next cue repeats it again for a moment, with word timings in
// between, so a line seen in the last few is not written again.
func vttToLRC(text string) string {
	var b strings.Builder
	var recent []string
	for _, block := range strings.Split(strings.ReplaceAll(text, "\r\n", "\n"), "\n\n") {
		lines := strings.Split(strings.TrimSpace(block), "\n")
		at := -1
		for i, l := range lines {
			if strings.Contains(l, "-->") {
				at = i
				break
			}
		}
		if at < 0 {
			continue // the header, NOTE or STYLE
		}
		start, ok := clockMS(strings.Fields(lines[at])[0])
		if !ok {
			continue
		}
		for _, l := range lines[at+1:] {
			l = strings.Join(strings.Fields(vttTag.ReplaceAllString(l, "")), " ")
			l = strings.NewReplacer("&amp;", "&", "&lt;", "<", "&gt;", ">", "&nbsp;", " ").Replace(l)
			if l == "" || contains(recent, l) {
				continue
			}
			b.WriteString(lrcTime(start) + l + "\n")
			recent = append(recent, l)
			if len(recent) > 3 {
				recent = recent[1:]
			}
		}
	}
	return b.String()
}

func contains(list []string, s string) bool {
	for _, x := range list {
		if x == s {
			return true
		}
	}
	return false
}

// mdTurn is one turn in a .transcript.md: **Speaker** (01:02:03): words.
var mdTurn = regexp.MustCompile(`^\*\*(.+?)\*\*\s*\((\d{1,2}(?::\d{2}){1,2})\):\s*(.*)$`)

// markdownToLRC keeps the speaker with each turn and drops the headings.
// It returns "" when the file has no timed turns.
func markdownToLRC(text string) string {
	var b strings.Builder
	for _, l := range strings.Split(text, "\n") {
		m := mdTurn.FindStringSubmatch(strings.TrimSpace(l))
		if m == nil {
			continue
		}
		ms, ok := clockMS(m[2])
		if !ok {
			continue
		}
		b.WriteString(lrcTime(ms) + m[1] + ": " + strings.TrimSpace(m[3]) + "\n")
	}
	return b.String()
}
