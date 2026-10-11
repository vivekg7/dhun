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
	var seen recentLines
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
			if seen.fresh(l) {
				b.WriteString(lrcTime(start) + l + "\n")
			}
		}
	}
	return b.String()
}

// recentLines is the last few lines of a transcript, for dropping a line
// that scrolling captions repeat.
type recentLines []string

// fresh says whether l is worth writing: not empty, and not among the last
// three lines written. It remembers l when it is.
func (r *recentLines) fresh(l string) bool {
	if l == "" {
		return false
	}
	for _, x := range *r {
		if x == l {
			return false
		}
	}
	*r = append(*r, l)
	if len(*r) > 3 {
		*r = (*r)[1:]
	}
	return true
}

var lrcLine = regexp.MustCompile(`^\s*(\[\d{1,3}:\d{2}(?:[.:]\d{1,3})?\])(.*)$`)

// spokenLRC drops from an episode's or a book's LRC the repeats and blank
// lines of scrolling captions. YouTube's automatic captions converted to
// LRC (by FFmpeg, say) write each line three times, as the vtt does, with
// a blank line between; a song's LRC is left as it is, since a chorus may
// well repeat a line straight after itself.
func spokenLRC(f SongFile, text string) string {
	if !f.Spoken {
		return text
	}
	var b strings.Builder
	var seen recentLines
	for _, l := range strings.Split(strings.ReplaceAll(text, "\r\n", "\n"), "\n") {
		m := lrcLine.FindStringSubmatch(l)
		if m == nil {
			if strings.TrimSpace(l) != "" {
				b.WriteString(l + "\n") // a header such as [ar:…]
			}
			continue
		}
		if words := strings.TrimSpace(m[2]); seen.fresh(words) {
			b.WriteString(m[1] + words + "\n")
		}
	}
	return b.String()
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
