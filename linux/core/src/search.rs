//! Search, the same in every box (docs/plans/029_search.md): the Mac's
//! `Search.swift` and the phone's `data/Search.kt` in Rust. Text folds into
//! lower-case words, each word has a sound key that evens out romanised
//! spelling, and a query matches an item when every one of its words
//! matches a word of the item. api/search-fold.tsv keeps the copies in step.

use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Lower-case words: Devanagari in Latin letters, accents off, "&" as "and", other punctuation splitting.
pub fn words(s: &str) -> Vec<String> {
    let t = spell(&devanagari(s).nfkd().collect::<String>().to_lowercase());
    let mut out = Vec::new();
    let mut w = String::new();
    for c in t.chars() {
        if is_combining_mark(c) {
            continue;
        }
        // Letters and digits, as Swift's letter and decimal-number categories.
        if c.is_alphanumeric() {
            w.push(c);
            continue;
        }
        if APOSTROPHES.contains(&c) {
            continue;
        }
        if !w.is_empty() {
            out.push(std::mem::take(&mut w));
        }
        if c == '&' {
            out.push("and".into());
        }
    }
    if !w.is_empty() {
        out.push(w);
    }
    out
}

const APOSTROPHES: [char; 5] = ['\'', '‘', '’', 'ʼ', '`'];

/// In order. "ch" is set aside while a lone "c" becomes "k".
const SOUNDS: [(&str, &str); 27] = [
    ("tsch", "ch"),
    ("tch", "ch"),
    ("sch", "sh"),
    ("ph", "f"),
    ("ck", "k"),
    ("ch", "\u{1}"),
    ("c", "k"),
    ("\u{1}", "ch"),
    ("q", "k"),
    ("w", "v"),
    ("z", "j"),
    ("kh", "k"),
    ("gh", "g"),
    ("th", "t"),
    ("dh", "d"),
    ("bh", "b"),
    ("jh", "j"),
    ("ue", "u"),
    ("oe", "o"),
    ("ee", "i"),
    ("oo", "u"),
    ("ie", "i"),
    ("ai", "e"),
    ("ei", "e"),
    ("ay", "e"),
    ("ey", "e"),
    ("y", "i"),
];

/// A word's sound key: Kabhi and Kabhie, Main and Mein, Pyar and Pyaar come out the same.
pub fn key(word: &str) -> String {
    let mut k = word.to_string();
    for (from, to) in SOUNDS {
        if k.contains(from) {
            k = k.replace(from, to);
        }
    }
    let mut b: Vec<char> = k.chars().collect();
    b.dedup();
    let n = b.len();
    if n > 2 && b[n - 1] == 'h' && "aeiou".contains(b[n - 2]) {
        b.pop();
    }
    b.into_iter().collect()
}

/// Letters that do not decompose into a base letter and an accent.
fn spell(s: &str) -> String {
    if !s.chars().any(|c| c as u32 >= 0xdf) {
        return s.to_string();
    }
    let mut b = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'ß' => b.push_str("ss"),
            'æ' => b.push_str("ae"),
            'œ' => b.push_str("oe"),
            'ø' => b.push('o'),
            'đ' | 'ð' => b.push('d'),
            'ł' => b.push('l'),
            'þ' => b.push_str("th"),
            'ı' => b.push('i'),
            _ => b.push(c),
        }
    }
    b
}

// Devanagari, U+0915 to U+0939.
const CONSONANTS: [&str; 37] = [
    "k", "kh", "g", "gh", "n", "ch", "chh", "j", "jh", "n", "t", "th", "d", "dh", "n", "t", "th", "d", "dh", "n", "n",
    "p", "ph", "b", "bh", "m", "y", "r", "r", "l", "l", "l", "v", "sh", "sh", "s", "h",
];
// The consonants with a nukta, U+0958 to U+095F.
const NUKTA_FORMS: [&str; 8] = ["q", "kh", "g", "z", "d", "dh", "f", "y"];
// Vowel signs, U+093E to U+094C, and vowels, U+0904 to U+0914.
const SIGNS: [&str; 15] = ["aa", "i", "ii", "u", "uu", "ri", "ri", "e", "e", "e", "ai", "o", "o", "o", "au"];
const VOWELS: [&str; 17] = ["a", "a", "aa", "i", "ii", "u", "uu", "ri", "l", "e", "e", "e", "ai", "o", "o", "o", "au"];

/// What a separate nukta does to a consonant.
fn nukta(latin: &str) -> Option<&'static str> {
    match latin {
        "k" => Some("q"),
        "j" => Some("z"),
        "ph" => Some("f"),
        _ => None,
    }
}

enum Piece {
    /// A consonant and its vowel: "a" until a sign, a virama ("") or the dropping of a final a says otherwise.
    Letter { latin: String, vowel: String, open: bool },
    /// The Latin of a vowel or sign.
    Text(String),
}

/// Devanagari in Latin letters. The inherent a is dropped where Hindi
/// drops it: at the end of a word, and between a vowel and a consonant
/// that has one (धड़कन is dhadkan). Worked from the right, so each letter
/// sees what was decided for the next.
pub fn devanagari(s: &str) -> String {
    if !s.chars().any(|c| ('\u{900}'..='\u{97f}').contains(&c)) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() * 2);
    let mut word: Vec<Piece> = Vec::new();
    fn flush(word: &mut Vec<Piece>, out: &mut String) {
        for i in (0..word.len()).rev() {
            let Piece::Letter { open: true, .. } = word[i] else { continue };
            let voweled = match i.checked_sub(1).map(|p| &word[p]) {
                Some(Piece::Text(_)) => true,
                Some(Piece::Letter { vowel, .. }) => !vowel.is_empty(),
                None => false,
            };
            let next = word.get(i + 1);
            let next_voweled = matches!(next, Some(Piece::Letter { vowel, .. }) if !vowel.is_empty());
            if i > 0 && (next.is_none() || (next_voweled && voweled)) {
                if let Piece::Letter { vowel, .. } = &mut word[i] {
                    vowel.clear();
                }
            }
        }
        for p in word.drain(..) {
            match p {
                Piece::Letter { latin, vowel, .. } => {
                    out.push_str(&latin);
                    out.push_str(&vowel);
                }
                Piece::Text(t) => out.push_str(&t),
            }
        }
    }
    fn letter(latin: &str) -> Piece {
        Piece::Letter { latin: latin.into(), vowel: "a".into(), open: true }
    }
    for c in s.chars() {
        let v = c as usize;
        let last = match word.last_mut() {
            Some(Piece::Letter { latin, vowel, open }) => Some((latin, vowel, open)),
            _ => None,
        };
        match v {
            0x915..=0x939 => word.push(letter(CONSONANTS[v - 0x915])),
            0x958..=0x95f => word.push(letter(NUKTA_FORMS[v - 0x958])),
            0x93c => {
                if let Some((latin, _, _)) = last {
                    if let Some(n) = nukta(latin) {
                        *latin = n.into();
                    }
                }
            }
            0x93e..=0x94c => {
                let sign = SIGNS[v - 0x93e];
                match last {
                    Some((_, vowel, open)) if *open => {
                        *vowel = sign.into();
                        *open = false;
                    }
                    _ => word.push(Piece::Text(sign.into())),
                }
            }
            0x94d => {
                if let Some((_, vowel, open)) = last {
                    vowel.clear();
                    *open = false;
                }
            }
            0x901..=0x903 => {
                // A nasal or visarga keeps the a before it: हंस is hans.
                if let Some((_, _, open)) = last {
                    *open = false;
                }
                word.push(Piece::Text(if v == 0x903 { "h" } else { "n" }.into()));
            }
            0x904..=0x914 => word.push(Piece::Text(VOWELS[v - 0x904].into())),
            0x960 => word.push(Piece::Text("ri".into())),
            0x961 => word.push(Piece::Text("l".into())),
            0x93d => {}
            0x950 => {
                flush(&mut word, &mut out);
                out.push_str("om");
            }
            0x966..=0x96f => {
                flush(&mut word, &mut out);
                out.push_str(&(v - 0x966).to_string());
            }
            _ => {
                flush(&mut word, &mut out);
                if v == 0x964 || v == 0x965 {
                    out.push(' ');
                } else {
                    out.push(c);
                }
            }
        }
    }
    flush(&mut word, &mut out);
    out
}

/// How well one query word matches one word; lower is better, `NO_MATCH` is not at all.
pub const NO_MATCH: i32 = i32::MAX;

/// One word of a query, perhaps narrowed to a field with `artist:` and the like.
pub struct Term {
    pub word: String,
    pub field: Option<String>,
    pub years: Option<(i64, i64)>,
    bytes: Vec<u8>,
    key: Vec<u8>,
    /// The key as the phone counts letters, for typos.
    key_units: Vec<u16>,
}

impl Term {
    fn new(word: String, field: Option<String>, years: Option<(i64, i64)>) -> Term {
        let k = key(&word);
        Term {
            bytes: word.as_bytes().to_vec(),
            key: k.as_bytes().to_vec(),
            key_units: k.encode_utf16().collect(),
            word,
            field,
            years,
        }
    }

    fn quality(&self, w: &[u8], k: &[u8]) -> i32 {
        if w == self.bytes {
            0
        } else if w.starts_with(&self.bytes) {
            1
        } else if has(w, &self.bytes) {
            2
        } else if !self.key.is_empty() && k.starts_with(&self.key) {
            3
        } else {
            NO_MATCH
        }
    }

    /// A typo away from the start of `k`, by sound: one edit in 4 to 7 letters, two from 8.
    /// `row` and `next` are scratch space, kept between calls.
    fn typo(&self, k: &[u16], row: &mut Vec<usize>, next: &mut Vec<usize>) -> bool {
        let m = self.key_units.len();
        if m < 4 {
            return false;
        }
        let most = if m >= 8 { 2 } else { 1 };
        if k.len() + most < m {
            return false;
        }
        row.clear();
        row.extend(0..=k.len());
        next.clear();
        next.resize(k.len() + 1, 0);
        for i in 1..=m {
            next[0] = i;
            let mut low = i;
            for j in 1..=k.len() {
                let sub = row[j - 1] + usize::from(self.key_units[i - 1] != k[j - 1]);
                next[j] = (row[j] + 1).min(next[j - 1] + 1).min(sub);
                low = low.min(next[j]);
            }
            if low > most {
                return false;
            }
            std::mem::swap(row, next);
        }
        row.iter().min().is_some_and(|&d| d <= most)
    }
}

fn has(a: &[u8], b: &[u8]) -> bool {
    if b.is_empty() {
        return true;
    }
    a.len() >= b.len() && a.windows(b.len()).any(|w| w[0] == b[0] && w == b)
}

/// What was typed: words, each perhaps narrowed to a field; `year:2010-2015` is a range.
pub struct Query {
    pub terms: Vec<Term>,
    /// The words with no field, as one string: a title that is all of them comes first.
    pub whole: String,
    /// What to look for in lyrics: the `lyrics:` words, or else every word if nothing is narrowed to another field.
    pub lyrics: String,
}

pub const FIELDS: [&str; 8] = ["title", "artist", "album", "composer", "genre", "year", "folder", "lyrics"];

impl Query {
    pub fn new(text: &str) -> Query {
        let mut terms = Vec::new();
        for (field, value, whole) in tokens(text) {
            let field = field.map(str::to_lowercase).filter(|f| FIELDS.contains(&f.as_str()));
            let value = unquote(if field.is_some() { value } else { whole });
            if field.as_deref() == Some("year") {
                if let Some((a, b)) = year_range(value) {
                    terms.push(Term::new(value.into(), field, Some((a.min(b), a.max(b)))));
                    continue;
                }
            }
            terms.extend(words(value).into_iter().map(|w| Term::new(w, field.clone(), None)));
        }
        let whole = terms.iter().filter(|t| t.field.is_none()).map(|t| t.word.as_str()).collect::<Vec<_>>().join(" ");
        let narrowed: Vec<&str> =
            terms.iter().filter(|t| t.field.as_deref() == Some("lyrics")).map(|t| t.word.as_str()).collect();
        let lyrics = if !narrowed.is_empty() {
            narrowed.join(" ")
        } else if terms.iter().any(|t| t.field.is_some()) {
            String::new()
        } else {
            whole.clone()
        };
        Query { terms, whole, lyrics }
    }

    /// Whether a line of lyrics holds every word, by the start of a word or
    /// its sound; no typos among thousands of lines.
    pub fn in_line(&self, words: &[String]) -> bool {
        let looked: Vec<&Term> =
            self.terms.iter().filter(|t| t.field.is_none() || t.field.as_deref() == Some("lyrics")).collect();
        if looked.is_empty() {
            return false;
        }
        let keys: Vec<String> = words.iter().map(|w| key(w)).collect();
        looked.iter().all(|t| words.iter().zip(&keys).any(|(w, k)| t.quality(w.as_bytes(), k.as_bytes()) <= 3))
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

/// The tokens of `(\p{L}+):("[^"]*"|\S+)|"[^"]*"|\S+`, as (field, its value, the whole match).
fn tokens(text: &str) -> Vec<(Option<&str>, &str, &str)> {
    let mut out = Vec::new();
    let mut rest = text;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return out;
        }
        let word_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let quoted = |s: &str| -> Option<usize> { s.strip_prefix('"')?.find('"').map(|i| i + 2) };
        // A field: letters, a colon, then a quoted phrase or a word.
        let letters = rest.find(|c: char| !c.is_alphabetic()).unwrap_or(rest.len());
        if letters > 0 && rest[letters..].starts_with(':') {
            let after = &rest[letters + 1..];
            let n = quoted(after).or_else(|| {
                let w = after.find(char::is_whitespace).unwrap_or(after.len());
                (w > 0).then_some(w)
            });
            if let Some(n) = n {
                let end = letters + 1 + n;
                out.push((Some(&rest[..letters]), &after[..n], &rest[..end]));
                rest = &rest[end..];
                continue;
            }
        }
        let end = quoted(rest).unwrap_or(word_end);
        out.push((None, &rest[..end], &rest[..end]));
        rest = &rest[end..];
    }
}

fn year_range(s: &str) -> Option<(i64, i64)> {
    let (a, b) = s.split_once("..").or_else(|| s.split_once('-'))?;
    let four = |x: &str| x.parse().ok().filter(|_| x.len() == 4 && x.bytes().all(|c| c.is_ascii_digit()));
    Some((four(a)?, four(b)?))
}

/// Kotlin's `removeSurrounding("\"")`.
fn unquote(s: &str) -> &str {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') { &s[1..s.len() - 1] } else { s }
}

/// Something searched for in an item. `name` is what a filter calls it
/// (`artist:`). A lower `weight` ranks a match higher; with none the field
/// is searched only through its filter.
pub struct Field<T> {
    pub name: &'static str,
    pub weight: Option<i32>,
    pub text: Box<dyn Fn(&T) -> String>,
}

pub fn field<T>(name: &'static str, weight: Option<i32>, text: impl Fn(&T) -> String + 'static) -> Field<T> {
    Field { name, weight, text: Box::new(text) }
}

/// The words of every item, each distinct word folded once. A query is
/// compared with the distinct words, then each item is a few lookups, which
/// keeps a keystroke over 100,000 songs within the 50 ms the targets allow
/// (plan 036). The per-item lists are flat arrays with offsets, not a list
/// per item and field: at 100,000 songs and eight fields that would be
/// 800,000 small allocations.
pub struct Index<T> {
    pub items: Vec<T>,
    names: Vec<&'static str>,
    weights: Vec<Option<i32>>,
    words: Packed<u8>,
    keys: Packed<u8>,
    key_units: Packed<u16>,
    /// Words in a field searched without a filter: only they decide that a query word needs no typo match.
    open: Vec<bool>,
    /// For item i and field f, its word ids are `cells[cell_at[i*F+f]..cell_at[i*F+f+1]]`.
    cells: Vec<u32>,
    cell_at: Vec<u32>,
    /// A field of two words or more, them run together, for "acdc"; an empty range for the rest.
    runs: Vec<u8>,
    run_at: Vec<u32>,
    /// The first field's text as folded, for a title that is the whole query.
    titles: Packed<u8>,
    /// The field named `year` as a number, 0 for none; empty when the items have no such field.
    years: Vec<i64>,
}

impl<T> Index<T> {
    pub fn new(items: Vec<T>, fields: Vec<Field<T>>) -> Index<T> {
        let nf = fields.len();
        let mut ids: HashMap<String, u32> = HashMap::new();
        let (mut words_, mut keys_, mut units) = (Packed::default(), Packed::default(), Packed::default());
        // An album's or artist's name comes once per song on it: fold each distinct text once.
        let mut folded: HashMap<String, Vec<String>> = HashMap::new();
        let mut cells = Vec::with_capacity(items.len() * nf * 2);
        let mut cell_at = Vec::with_capacity(items.len() * nf + 1);
        let mut runs = Vec::new();
        let mut run_at = Vec::with_capacity(items.len() * nf + 1);
        let mut titles = Packed::default();
        let year_field = fields.iter().position(|f| f.name == "year");
        let mut years = Vec::new();
        let mut open_ids = vec![];
        cell_at.push(0);
        run_at.push(0);
        for item in &items {
            for (f, fld) in fields.iter().enumerate() {
                let t = (fld.text)(item);
                let ws = folded.entry(t).or_insert_with_key(|t| words(t));
                for w in ws.iter() {
                    let id = *ids.entry(w.clone()).or_insert_with(|| {
                        words_.push(w.as_bytes());
                        let k = key(w);
                        keys_.push(k.as_bytes());
                        units.push(&k.encode_utf16().collect::<Vec<_>>());
                        open_ids.push(false);
                        (words_.len() - 1) as u32
                    });
                    if fld.weight.is_some() {
                        open_ids[id as usize] = true;
                    }
                    cells.push(id);
                }
                cell_at.push(cells.len() as u32);
                if ws.len() >= 2 {
                    for w in ws.iter() {
                        runs.extend_from_slice(w.as_bytes());
                    }
                }
                run_at.push(runs.len() as u32);
                if f == 0 {
                    titles.push(ws.join(" ").as_bytes());
                }
                if Some(f) == year_field {
                    years.push(ws.first().and_then(|w| w.parse().ok()).unwrap_or(0));
                }
            }
        }
        for v in [&mut cells, &mut cell_at, &mut run_at] {
            v.shrink_to_fit();
        }
        runs.shrink_to_fit();
        Index {
            items,
            names: fields.iter().map(|f| f.name).collect(),
            weights: fields.iter().map(|f| f.weight).collect(),
            key_units: units.done(),
            keys: keys_.done(),
            words: words_.done(),
            open: open_ids,
            cells,
            cell_at,
            runs,
            run_at,
            titles: titles.done(),
            years,
        }
    }

    /// The positions of what `q` matches, in the items' own order.
    pub fn filter(&self, q: &Query) -> Vec<usize> {
        if q.is_empty() {
            return (0..self.items.len()).collect();
        }
        let s = self.scores(q);
        (0..self.items.len()).filter(|&i| s[i] != NO_MATCH).collect()
    }

    /// What `q` matches as (position, score), best (lowest) first. Among
    /// equal scores a higher `boost` comes first: the user's favourites and
    /// most played, never above a better match.
    pub fn search(&self, q: &Query, boost: impl Fn(&T) -> i64) -> Vec<(usize, i32)> {
        if q.is_empty() {
            return vec![];
        }
        let s = self.scores(q);
        let mut out: Vec<(usize, i32, i64)> = Vec::new();
        for (i, &score) in s.iter().enumerate() {
            if score == NO_MATCH {
                continue;
            }
            let title = self.titles.get(i);
            let bonus = if q.whole.is_empty() {
                0
            } else if title == q.whole.as_bytes() {
                20
            } else if title.starts_with(q.whole.as_bytes()) {
                10
            } else {
                0
            };
            out.push((i, score - bonus, boost(&self.items[i])));
        }
        out.sort_by_key(|&(i, score, b)| (score, -b, i));
        out.into_iter().map(|(i, s, _)| (i, s)).collect()
    }

    fn scores(&self, q: &Query) -> Vec<i32> {
        let nf = self.names.len();
        let qualities: Vec<Option<Vec<i32>>> = q
            .terms
            .iter()
            .map(|t| {
                if t.years.is_some() {
                    return None;
                }
                let mut a: Vec<i32> =
                    (0..self.words.len()).map(|i| t.quality(self.words.get(i), self.keys.get(i))).collect();
                // Typos only for a word that matches nothing better anywhere: "love" must not bring "live".
                if !(0..a.len()).any(|i| a[i] <= 3 && (self.open[i] || t.field.is_some())) {
                    let (mut row, mut next) = (Vec::new(), Vec::new());
                    for (i, q) in a.iter_mut().enumerate() {
                        if t.typo(self.key_units.get(i), &mut row, &mut next) {
                            *q = 4;
                        }
                    }
                }
                Some(a)
            })
            .collect();
        // Each term's weight in each field, or -1 where it is not looked for.
        let plan: Vec<Vec<i32>> = q
            .terms
            .iter()
            .map(|t| {
                (0..nf)
                    .map(|f| match &t.field {
                        Some(name) if name == self.names[f] => self.weights[f].unwrap_or(0),
                        Some(_) => -1,
                        None => self.weights[f].unwrap_or(-1),
                    })
                    .collect()
            })
            .collect();
        (0..self.items.len())
            .map(|i| {
                let mut total = 0;
                for (n, t) in q.terms.iter().enumerate() {
                    let mut best = NO_MATCH;
                    for (f, &w) in plan[n].iter().enumerate() {
                        if w < 0 {
                            continue;
                        }
                        if let Some((lo, hi)) = t.years {
                            // Only the field named year is in the plan for a year term.
                            if !self.years.is_empty() && (lo..=hi).contains(&self.years[i]) {
                                best = best.min(w);
                            }
                            continue;
                        }
                        let a = qualities[n].as_ref().unwrap();
                        let c = i * nf + f;
                        for &id in &self.cells[self.cell_at[c] as usize..self.cell_at[c + 1] as usize] {
                            let quality = a[id as usize];
                            if quality != NO_MATCH {
                                best = best.min(quality * 10 + w);
                            }
                        }
                        // The field's words run together: "acdc" finds AC/DC.
                        if best > 20 + w && t.bytes.len() >= 3 {
                            let run = &self.runs[self.run_at[c] as usize..self.run_at[c + 1] as usize];
                            if !run.is_empty() && has(run, &t.bytes) {
                                best = 20 + w;
                            }
                        }
                    }
                    if best == NO_MATCH {
                        return NO_MATCH;
                    }
                    total += best;
                }
                total
            })
            .collect()
    }
}

/// Many short slices in one allocation: the index holds tens of thousands
/// of words, and an allocation each costs memory and scatters it, so what a
/// build frees cannot go back to the system.
struct Packed<T> {
    data: Vec<T>,
    at: Vec<u32>,
}

impl<T> Default for Packed<T> {
    fn default() -> Self {
        Packed { data: Vec::new(), at: vec![0] }
    }
}

impl<T: Copy> Packed<T> {
    fn push(&mut self, s: &[T]) {
        self.data.extend_from_slice(s);
        self.at.push(self.data.len() as u32);
    }
    fn get(&self, i: usize) -> &[T] {
        &self.data[self.at[i] as usize..self.at[i + 1] as usize]
    }
    fn len(&self) -> usize {
        self.at.len() - 1
    }
    /// Gives back what the build reserved beyond its needs.
    fn done(mut self) -> Self {
        self.data.shrink_to_fit();
        self.at.shrink_to_fit();
        self
    }
}

/// A list searched by its names alone, as artists, genres or playlists; `field` is its filter.
pub fn names<T>(items: Vec<T>, field_name: &'static str, name: impl Fn(&T) -> String + 'static) -> Index<T> {
    Index::new(items, vec![field(field_name, Some(0), name)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Song;
    use std::sync::Arc;

    fn song(id: i64, title: &str, artist: &str, album: &str, composer: &str, year: i64) -> Arc<Song> {
        Arc::new(Song {
            id,
            path: format!("a/{id}.mp3"),
            title: title.into(),
            artist: artist.into(),
            artists: if artist.is_empty() { vec![] } else { vec![artist.into()] },
            album: album.into(),
            composer: composer.into(),
            year,
            ..Default::default()
        })
    }

    fn index() -> Index<Arc<Song>> {
        crate::catalog::song_index(vec![
            song(1, "Tum Hi Ho", "Arijit Singh", "Aashiqui 2", "", 2013),
            song(2, "Live and Let Die", "Wings", "", "", 0),
            song(3, "Love Me Do", "The Beatles", "", "", 0),
            song(4, "दिल से", "A.R. Rahman", "दिल से", "A.R. Rahman", 1998),
            song(5, "Highway to Hell", "AC/DC", "", "", 0),
            song(6, "Kabhie Kabhie", "Mukesh", "", "", 1976),
            song(7, "Hello", "Adele", "Tum Hi Ho Hits", "", 0),
        ])
    }

    fn find(q: &str) -> Vec<i64> {
        let ix = index();
        ix.search(&Query::new(q), |_| 0).into_iter().map(|(i, _)| ix.items[i].id).collect()
    }

    // The phone's, the Mac's, the web's and the server's copies read the same file.
    #[test]
    fn folds_as_the_other_copies_do() {
        let file = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../api/search-fold.tsv")).unwrap();
        let lines: Vec<&str> = file.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
        assert!(lines.len() > 20);
        for line in lines {
            let cols: Vec<&str> = line.split('\t').collect();
            assert_eq!(words(cols[0]).join(" "), cols[1], "{}", cols[0]);
            assert_eq!(words(cols[0]).iter().map(|w| key(w)).collect::<Vec<_>>().join(" "), cols[2], "{}", cols[0]);
        }
    }

    #[test]
    fn words_match_across_fields_in_any_order() {
        assert_eq!(find("arijit tum"), [1]);
        assert_eq!(find("ho tum arijit"), [1]);
    }

    #[test]
    fn the_title_ranks_above_the_album() {
        // Song 7 only has it in its album name.
        assert_eq!(find("tum hi ho"), [1, 7]);
    }

    #[test]
    fn typos_only_when_nothing_better_matches() {
        assert_eq!(find("arjit"), [1]);
        // "love" matches a word, so "live" is not offered as a typo.
        assert_eq!(find("love"), [3]);
    }

    #[test]
    fn spelling_and_scripts_meet() {
        assert_eq!(find("dil se"), [4]);
        assert_eq!(find("kabhi"), [6]);
        assert_eq!(find("acdc"), [5]);
    }

    #[test]
    fn filters_narrow_a_word_to_a_field() {
        assert_eq!(find("composer:rahman"), [4]);
        assert!(find("title:arijit").is_empty());
        let mut years = find("year:1970-2000");
        years.sort_unstable();
        assert_eq!(years, [4, 6]);
        assert_eq!(find("artist:\"arijit singh\" tum"), [1]);
    }
}
