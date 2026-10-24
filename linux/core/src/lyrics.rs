//! Lyrics (plan 015), as the server stores them: a sibling `.lrc` file or
//! embedded tags. Every copy fetched is kept in the database, so the
//! lyrics of a downloaded song, or of one already viewed, show offline too.
//! The parser is the phone's, in our own words.

use crate::api::{self, Api};
use crate::db::Db;
use crate::model::Song;
use crate::search::{Query, words};
use std::sync::Mutex;

/// One line; `ms` is where it starts, or -1 in lyrics without times.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub ms: i64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Parsed {
    pub synced: bool,
    pub lines: Vec<Line>,
}

impl Parsed {
    /// The line playing at `ms`: the last one started, or None before the first.
    pub fn at(&self, ms: i64) -> Option<usize> {
        if !self.synced {
            return None;
        }
        self.lines.iter().rposition(|l| l.ms <= ms)
    }
}

/// A `[mm:ss.xx]` stamp (or `<…>` with `open` '<') at the start of `s`:
/// its time and its length.
fn stamp(s: &str, open: char, close: char) -> Option<(i64, usize)> {
    let rest = s.strip_prefix(open)?;
    let end = rest.find(close)?;
    let inner = &rest[..end];
    let (m, sec) = inner.split_once(':')?;
    let (sec, frac) = match sec.find(['.', ':']) {
        Some(i) => (&sec[..i], &sec[i + 1..]),
        None => (sec, ""),
    };
    let digits = |x: &str, max: usize| !x.is_empty() && x.len() <= max && x.bytes().all(|b| b.is_ascii_digit());
    if !digits(m, 3) || !digits(sec, 2) || !(frac.is_empty() || digits(frac, 3)) {
        return None;
    }
    // Hundredths are tenths of a second's tens: "5" is 500 ms, "25" 250.
    let ms = format!("{frac:0<3}").parse::<i64>().unwrap_or(0);
    Some((m.parse::<i64>().ok()? * 60_000 + sec.parse::<i64>().ok()? * 1000 + ms, end + 2))
}

/// `[ar:Someone]`, `[offset:+500]`: a tag's name and value, for a whole line.
fn tag(line: &str) -> Option<(&str, &str)> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    let (name, value) = inner.split_once(':')?;
    (!name.is_empty() && name.bytes().all(|b| b.is_ascii_alphabetic() || b == b'#')).then_some((name, value))
}

/// Word stamps (`<mm:ss.xx>`, enhanced LRC) taken out.
fn without_word_stamps(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        match stamp(&rest[i..], '<', '>') {
            Some((_, n)) => rest = &rest[i + n..],
            None => {
                out.push('<');
                rest = &rest[i + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// LRC: `[mm:ss.xx]` stamps, several on one line for a repeated chorus,
/// `[offset:±ms]`, and word stamps, which are dropped. Text with no stamps
/// is shown as plain lyrics, without its `[ar:…]`-style tags.
pub fn parse(text: &str) -> Parsed {
    let mut offset = 0;
    let mut timed: Vec<Line> = vec![];
    let mut plain: Vec<String> = vec![];
    for raw in text.lines() {
        let mut line = raw.trim();
        if let Some((name, value)) = tag(line).filter(|_| stamp(line, '[', ']').is_none()) {
            if name.eq_ignore_ascii_case("offset") {
                offset = value.trim().trim_start_matches('+').parse().unwrap_or(0);
            }
            continue;
        }
        let mut times = vec![];
        while let Some((ms, n)) = stamp(line, '[', ']') {
            times.push(ms);
            line = &line[n..];
        }
        let clean = without_word_stamps(line).trim().to_string();
        if times.is_empty() {
            plain.push(clean);
        } else {
            timed.extend(times.into_iter().map(|ms| Line { ms, text: clean.clone() }));
        }
    }
    if timed.is_empty() {
        // Blank lines at either end are file layout, not verses.
        let first = plain.iter().position(|l| !l.is_empty()).unwrap_or(plain.len());
        let last = plain.iter().rposition(|l| !l.is_empty()).map_or(first, |i| i + 1);
        let lines = plain[first..last].iter().map(|t| Line { ms: -1, text: t.clone() }).collect();
        return Parsed { synced: false, lines };
    }
    // A positive offset shows the lines earlier. Stable: a chorus keeps its place among equal times.
    for l in &mut timed {
        l.ms = (l.ms - offset).max(0);
    }
    timed.sort_by_key(|l| l.ms);
    Parsed { synced: true, lines: timed }
}

/// Fetches and keeps a song's lyrics; None (and forgotten) when the server has none.
pub fn fetch(api: &Api, db: &Mutex<Db>, song: i64) -> api::Result<Option<String>> {
    match api.lyrics(song) {
        Ok(text) => {
            let _ = db.lock().unwrap().put_lyrics(song, &text);
            Ok(Some(text))
        }
        Err(api::Error::Http { code: 404, .. }) => {
            let _ = db.lock().unwrap().delete_lyrics(song);
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// For downloads: the lyrics of `songs` not yet kept. Lyrics are small, so
/// this runs on any network; it stops at the first failure.
pub fn keep(api: &Api, db: &Mutex<Db>, songs: &[impl std::ops::Deref<Target = Song>]) {
    let kept = db.lock().unwrap().lyrics_songs().unwrap_or_default();
    for s in songs.iter().filter(|s| s.has_lyrics && !kept.contains(&s.id)) {
        if fetch(api, db, s.id).is_err() {
            return;
        }
    }
}

/// A line of lyrics found: the song and the line, as the server answers.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Hit {
    pub song: i64,
    pub line: String,
}

/// At most this many songs found in lyrics, here and on the server.
pub const HITS: usize = 50;

/// Songs whose kept lyrics hold `q`'s words, for when the server cannot
/// search them: a line with the words as typed first.
pub fn search_kept(rows: &[(i64, String)], q: &Query) -> Vec<Hit> {
    if q.lyrics.is_empty() {
        return vec![];
    }
    let mut hits: Vec<(Hit, bool)> = vec![];
    for (song, text) in rows {
        for l in parse(text).lines {
            let w = words(&l.text);
            if q.in_line(&w) {
                hits.push((Hit { song: *song, line: l.text.trim().into() }, w.join(" ").contains(&q.lyrics)));
                break;
            }
        }
    }
    hits.sort_by_key(|h| !h.1);
    hits.into_iter().take(HITS).map(|h| h.0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(ms: i64, text: &str) -> Line {
        Line { ms, text: text.into() }
    }

    // Lines as the curation workflow's .lrc files have them: tags, a repeated
    // chorus on one line, two- and three-digit fractions, word stamps.
    #[test]
    fn synced_lines_are_timed_sorted_and_clean() {
        let p = parse(
            "[ar:Someone]\n[ti:Saans]\n[00:05.5]First\n[00:10.25][00:30.250]Chorus\n\
             [00:20.00]<00:20.00>Word <00:21.50>stamps\n[00:25.00]",
        );
        assert_eq!(
            p.lines,
            [
                line(5_500, "First"),
                line(10_250, "Chorus"),
                line(20_000, "Word stamps"),
                line(25_000, ""),
                line(30_250, "Chorus")
            ]
        );
        assert_eq!(p.at(5_000), None);
        assert_eq!(p.at(19_999), Some(1));
        assert_eq!(p.at(99_000), Some(4));
    }

    #[test]
    fn an_offset_shows_lines_earlier() {
        let p = parse("[offset:+500]\n[00:01.00]a\n[00:00.20]b");
        assert_eq!(p.lines, [line(0, "b"), line(500, "a")]);
    }

    // Embedded lyrics are often plain; a stray tag must not make them "synced" or show up as a verse.
    #[test]
    fn plain_lyrics_keep_their_lines_but_not_tags() {
        let p = parse("\n[ar:Someone]\nOne\n\nTwo\n\n");
        assert!(!p.synced);
        assert_eq!(p.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["One", "", "Two"]);
        assert_eq!(p.at(10_000), None);
    }
}
