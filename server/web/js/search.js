// Search, the same in every box (docs/plans/029_search.md): the Mac's
// Search.swift and the phone's data/Search.kt in JavaScript. Text folds into
// lower-case words, each word has a sound key that evens out romanised
// spelling, and a query matches an item when every one of its words matches a
// word of the item. api/search-fold.tsv keeps the copies in step.

const apostrophes = new Set(["'", "‘", "’", "ʼ", "`"]);
const mark = /\p{M}/u;
const wordChar = /[\p{L}\p{Nd}]/u;

/** Lower-case words: Devanagari in Latin letters, accents off, "&" as "and", other punctuation splitting. */
export function words(s) {
  const t = spell(devanagari(s).normalize("NFKD").toLowerCase());
  const out = [];
  let w = "";
  for (const c of t) {
    if (mark.test(c)) continue;
    if (wordChar.test(c)) {
      w += c;
      continue;
    }
    if (apostrophes.has(c)) continue;
    if (w) out.push(w);
    w = "";
    if (c === "&") out.push("and");
  }
  if (w) out.push(w);
  return out;
}

/** The words of `s` joined by single spaces. */
export const text = (s) => words(s).join(" ");

// In order. "ch" is set aside while a lone "c" becomes "k".
const sounds = [
  ["tsch", "ch"],
  ["tch", "ch"],
  ["sch", "sh"],
  ["ph", "f"],
  ["ck", "k"],
  ["ch", "\u0001"],
  ["c", "k"],
  ["\u0001", "ch"],
  ["q", "k"],
  ["w", "v"],
  ["z", "j"],
  ["kh", "k"],
  ["gh", "g"],
  ["th", "t"],
  ["dh", "d"],
  ["bh", "b"],
  ["jh", "j"],
  ["ue", "u"],
  ["oe", "o"],
  ["ee", "i"],
  ["oo", "u"],
  ["ie", "i"],
  ["ai", "e"],
  ["ei", "e"],
  ["ay", "e"],
  ["ey", "e"],
  ["y", "i"],
];

/** A word's sound key: Kabhi and Kabhie, Main and Mein, Pyar and Pyaar come out the same. */
export function key(word) {
  let k = word;
  // split and join replace left to right without overlaps, as Kotlin's `replace`.
  for (const [from, to] of sounds) if (k.includes(from)) k = k.split(from).join(to);
  const b = [];
  for (const c of k) if (b[b.length - 1] !== c) b.push(c);
  if (b.length > 2 && b[b.length - 1] === "h" && "aeiou".includes(b[b.length - 2]))
    b.pop();
  return b.join("");
}

const spelled = {
  ß: "ss",
  æ: "ae",
  œ: "oe",
  ø: "o",
  đ: "d",
  ð: "d",
  ł: "l",
  þ: "th",
  ı: "i",
};

/** Letters that do not decompose into a base letter and an accent. */
function spell(s) {
  if (!/[ß-\u{10ffff}]/u.test(s)) return s;
  let b = "";
  for (const c of s) b += spelled[c] ?? c;
  return b;
}

// Devanagari, U+0915 to U+0939.
// prettier-ignore
const consonants = [
  "k", "kh", "g", "gh", "n", "ch", "chh", "j", "jh", "n", "t", "th", "d", "dh", "n", "t", "th", "d",
  "dh", "n", "n", "p", "ph", "b", "bh", "m", "y", "r", "r", "l", "l", "l", "v", "sh", "sh", "s", "h",
];
// The consonants with a nukta, U+0958 to U+095F, and what a separate nukta does to a consonant.
const nuktaForms = ["q", "kh", "g", "z", "d", "dh", "f", "y"];
const nukta = { k: "q", j: "z", ph: "f" };
// Vowel signs, U+093E to U+094C, and vowels, U+0904 to U+0914.
const signs = [
  "aa",
  "i",
  "ii",
  "u",
  "uu",
  "ri",
  "ri",
  "e",
  "e",
  "e",
  "ai",
  "o",
  "o",
  "o",
  "au",
];
// prettier-ignore
const vowels = [
  "a", "a", "aa", "i", "ii", "u", "uu", "ri", "l", "e", "e", "e", "ai", "o", "o", "o", "au",
];

/**
 * Devanagari in Latin letters. The inherent a is dropped where Hindi drops
 * it: at the end of a word, and between a vowel and a consonant that has one
 * (धड़कन is dhadkan). Worked from the right, so each letter sees what was
 * decided for the next. A piece is a letter ({latin, vowel, open}) or the
 * Latin of a vowel or sign (a string).
 */
export function devanagari(s) {
  if (!/[ऀ-ॿ]/.test(s)) return s;
  let out = "";
  let word = [];
  const isLetter = (p) => p != null && typeof p === "object";
  const flush = () => {
    for (let i = word.length - 1; i >= 0; i--) {
      const l = word[i];
      if (!isLetter(l) || !l.open) continue;
      const next = word[i + 1];
      const prev = word[i - 1];
      const voweled = prev == null ? false : isLetter(prev) ? prev.vowel !== "" : true;
      const nextVoweled = isLetter(next) && next.vowel !== "";
      if (i > 0 && (next == null || (nextVoweled && voweled))) l.vowel = "";
    }
    for (const p of word) out += isLetter(p) ? p.latin + p.vowel : p;
    word = [];
  };
  for (const c of s) {
    const last = isLetter(word[word.length - 1]) ? word[word.length - 1] : null;
    const v = c.codePointAt(0);
    if (v >= 0x915 && v <= 0x939)
      word.push({ latin: consonants[v - 0x915], vowel: "a", open: true });
    else if (v >= 0x958 && v <= 0x95f)
      word.push({ latin: nuktaForms[v - 0x958], vowel: "a", open: true });
    else if (v === 0x93c) {
      if (last) last.latin = nukta[last.latin] ?? last.latin;
    } else if (v >= 0x93e && v <= 0x94c) {
      const sign = signs[v - 0x93e];
      if (last && last.open) {
        last.vowel = sign;
        last.open = false;
      } else word.push(sign);
    } else if (v === 0x94d) {
      if (last) {
        last.vowel = "";
        last.open = false;
      }
    } else if (v === 0x901 || v === 0x902 || v === 0x903) {
      // A nasal or visarga keeps the a before it: हंस is hans.
      if (last) last.open = false;
      word.push(v === 0x903 ? "h" : "n");
    } else if (v >= 0x904 && v <= 0x914) word.push(vowels[v - 0x904]);
    else if (v === 0x960) word.push("ri");
    else if (v === 0x961) word.push("l");
    else if (v === 0x93d) {
      // avagraha: silent
    } else if (v === 0x950) {
      flush();
      out += "om";
    } else if (v >= 0x966 && v <= 0x96f) {
      flush();
      out += String(v - 0x966);
    } else {
      flush();
      out += v === 0x964 || v === 0x965 ? " " : c;
    }
  }
  flush();
  return out;
}

/** How well one query word matches one word; lower is better. */
const noMatch = Infinity;

const utf8 = new TextEncoder();

class Term {
  constructor(word, field = null, years = null) {
    this.word = word;
    this.field = field;
    this.years = years;
    this.key = key(word);
    this.long = utf8.encode(word).length >= 3;
  }

  quality(w, k) {
    if (w === this.word) return 0;
    if (w.startsWith(this.word)) return 1;
    if (w.includes(this.word)) return 2;
    if (this.key && k.startsWith(this.key)) return 3;
    return noMatch;
  }

  /** A typo away from the start of `k`, by sound: one edit in 4 to 7 letters, two from 8. */
  typo(k) {
    const a = this.key;
    const m = a.length;
    if (m < 4) return false;
    const most = m >= 8 ? 2 : 1;
    if (k.length < m - most) return false;
    let row = Array.from({ length: k.length + 1 }, (_, j) => j);
    let next = new Array(k.length + 1);
    for (let i = 1; i <= m; i++) {
      next[0] = i;
      let low = i;
      for (let j = 1; j <= k.length; j++) {
        next[j] = Math.min(
          row[j] + 1,
          next[j - 1] + 1,
          row[j - 1] + (a[i - 1] === k[j - 1] ? 0 : 1),
        );
        low = Math.min(low, next[j]);
      }
      if (low > most) return false;
      [row, next] = [next, row];
    }
    return Math.min(...row) <= most;
  }
}

const fields = new Set([
  "title",
  "artist",
  "album",
  "composer",
  "genre",
  "year",
  "folder",
  "lyrics",
]);
const unquote = (s) =>
  s.length >= 2 && s.startsWith('"') && s.endsWith('"') ? s.slice(1, -1) : s;

/**
 * What was typed: words, each perhaps narrowed to a field with `artist:`,
 * `album:` and the like; `year:2010-2015` is a range.
 */
export class Query {
  constructor(input) {
    const terms = [];
    for (const m of input.matchAll(/(\p{L}+):("[^"]*"|\S+)|"[^"]*"|\S+/gu)) {
      const f = m[1]?.toLowerCase();
      const field = f && fields.has(f) ? f : null;
      const value = unquote(field ? m[2] : m[0]);
      const r = field === "year" && value.match(/^(\d{4})(?:-|\.\.)(\d{4})$/);
      if (r) {
        const [a, b] = [Number(r[1]), Number(r[2])];
        terms.push(new Term(value, field, [Math.min(a, b), Math.max(a, b)]));
      } else {
        for (const w of words(value)) terms.push(new Term(w, field));
      }
    }
    this.terms = terms;
    /** The words with no field, as one string: a title that is all of them comes first. */
    this.whole = terms
      .filter((t) => !t.field)
      .map((t) => t.word)
      .join(" ");
    const narrowed = terms.filter((t) => t.field === "lyrics");
    /** What to look for in lyrics: the `lyrics:` words, or else every word if nothing is narrowed to another field. */
    this.lyrics = narrowed.length
      ? narrowed.map((t) => t.word).join(" ")
      : terms.some((t) => t.field)
        ? ""
        : this.whole;
  }

  get empty() {
    return this.terms.length === 0;
  }
}

/**
 * The words of every item, each distinct word folded once. A query is
 * compared with the distinct words, then each item is a few lookups, which
 * keeps a keystroke over 7,000 songs quick. `fields` are
 * `[name, weight, text]`: a lower weight ranks a match higher; with a null
 * weight the field is searched only through its filter.
 */
export class Index {
  constructor(items, fieldList) {
    this.items = items;
    this.names = fieldList.map((f) => f[0]);
    this.weights = fieldList.map((f) => f[1]);
    const ids = new Map();
    const ws = [];
    this.texts = items.map((item) => fieldList.map((f) => text(f[2](item))));
    this.cells = this.texts.map((item) =>
      item.map((t) =>
        t === ""
          ? []
          : t.split(" ").map((w) => {
              let id = ids.get(w);
              if (id === undefined) {
                id = ws.length;
                ws.push(w);
                ids.set(w, id);
              }
              return id;
            }),
      ),
    );
    this.words = ws;
    this.keys = ws.map(key);
    // Words in a field searched without a filter: only they decide that a query word needs no typo match.
    this.open = new Array(ws.length).fill(false);
    for (const item of this.cells)
      item.forEach((cell, f) => {
        if (this.weights[f] != null) for (const id of cell) this.open[id] = true;
      });
    // Each field's text run together, for "acdc".
    this.runs = this.texts.map((item) => item.map((t) => t.replaceAll(" ", "")));
  }

  /** What `q` matches, in the items' own order. */
  filter(q) {
    if (q.empty) return this.items;
    const s = this.scores(q);
    return this.items.filter((_, i) => s[i] !== noMatch);
  }

  /**
   * What `q` matches with its score, best (lowest) first. Among equal scores a
   * higher `boost` comes first: the user's favourites and most played, never
   * above a better match.
   */
  search(q, boost = () => 0) {
    if (q.empty) return [];
    const s = this.scores(q);
    const out = [];
    for (let i = 0; i < this.items.length; i++) {
      if (s[i] === noMatch) continue;
      const title = this.texts[i][0];
      const bonus = !q.whole
        ? 0
        : title === q.whole
          ? 20
          : title.startsWith(q.whole)
            ? 10
            : 0;
      out.push({ i, score: s[i] - bonus, boost: boost(this.items[i]) });
    }
    out.sort((a, b) => a.score - b.score || b.boost - a.boost || a.i - b.i);
    return out.map((o) => ({ item: this.items[o.i], score: o.score }));
  }

  scores(q) {
    const qualities = q.terms.map((t) => {
      if (t.years) return null;
      const a = this.words.map((w, i) => t.quality(w, this.keys[i]));
      // Typos only for a word that matches nothing better anywhere: "love" must not bring "live".
      if (!a.some((v, i) => v <= 3 && (this.open[i] || t.field))) {
        for (let i = 0; i < a.length; i++) if (t.typo(this.keys[i])) a[i] = 4;
      }
      return a;
    });
    // Each term's weight in each field, or -1 where it is not looked for.
    const plan = q.terms.map((t) =>
      this.names.map((name, f) => {
        if (t.field) return t.field === name ? (this.weights[f] ?? 0) : -1;
        return this.weights[f] ?? -1;
      }),
    );
    return this.items.map((_, i) => {
      let total = 0;
      for (let n = 0; n < q.terms.length; n++) {
        const t = q.terms[n];
        let best = noMatch;
        plan[n].forEach((w, f) => {
          if (w < 0) return;
          if (t.years) {
            const y = Number(this.texts[i][f]);
            if (this.texts[i][f] !== "" && y >= t.years[0] && y <= t.years[1])
              best = Math.min(best, w);
            return;
          }
          const a = qualities[n];
          for (const id of this.cells[i][f])
            if (a[id] !== noMatch) best = Math.min(best, a[id] * 10 + w);
          // The field's words run together: "acdc" finds AC/DC.
          if (best > 20 + w && t.long && this.runs[i][f].includes(t.word))
            best = 20 + w;
        });
        if (best === noMatch) return noMatch;
        total += best;
      }
      return total;
    });
  }
}

/** A song's fields, most telling first (docs/plans/029_search.md). */
export const songIndex = (songs) =>
  new Index(songs, [
    ["title", 0, (s) => s.title],
    ["artist", 1, (s) => [...s.artists, s.artist ?? ""].join(" ")],
    ["album", 2, (s) => s.album ?? ""],
    ["artist", 3, (s) => s.albumArtist ?? ""],
    ["composer", 3, (s) => s.composer ?? ""],
    ["genre", 4, (s) => s.genres.join(" ")],
    ["year", 4, (s) => (s.year > 0 ? String(s.year) : "")],
    ["folder", null, (s) => s.folder],
  ]);

/** A list searched by its names alone, as artists, genres or playlists; `field` is its filter. */
export const nameIndex = (items, field, name) => new Index(items, [[field, 0, name]]);

/** At most this many songs found in lyrics, here and on the server. */
export const lyricsHits = 50;
