import Foundation

/// Search, the same in every box (docs/plans/029_search.md), the phone's
/// `data/Search.kt` in Swift. Text folds into lower-case words, each word has
/// a sound key that evens out romanised spelling, and a query matches an
/// item when every one of its words matches a word of the item. The phone
/// and the server fold the same way, and api/search-fold.tsv keeps the
/// three copies in step.
public enum Fold {
    /// Lower-case words: Devanagari in Latin letters, accents off, "&" as "and", other punctuation splitting.
    public static func words(_ s: String) -> [String] {
        let t = spell(devanagari(s).decomposedStringWithCompatibilityMapping.lowercased())
        var out: [String] = []
        var w = String.UnicodeScalarView()
        func end() {
            if !w.isEmpty { out.append(String(w)) }
            w.removeAll()
        }
        for c in t.unicodeScalars {
            switch c.properties.generalCategory {
            case .nonspacingMark, .spacingMark, .enclosingMark: continue
            case .uppercaseLetter, .lowercaseLetter, .titlecaseLetter, .modifierLetter, .otherLetter,
                .decimalNumber:
                w.append(c)
            default:
                if apostrophes.contains(c) { continue }
                end()
                if c == "&" { out.append("and") }
            }
        }
        end()
        return out
    }

    /// The words of `s` joined by single spaces.
    public static func text(_ s: String) -> String { words(s).joined(separator: " ") }

    /// A word's sound key: Kabhi and Kabhie, Main and Mein, Pyar and Pyaar come out the same.
    public static func key(_ word: String) -> String {
        var k = Array(word.unicodeScalars)
        for (from, to) in sounds { k = replace(k, from, to) }
        var b: [Unicode.Scalar] = []
        for c in k where b.last != c { b.append(c) }
        if b.count > 2, b[b.count - 1] == "h", "aeiou".unicodeScalars.contains(b[b.count - 2]) {
            b.removeLast()
        }
        return String(String.UnicodeScalarView(b))
    }

    private static let apostrophes = Set("'‘’ʼ`".unicodeScalars)

    /// In order. "ch" is set aside while a lone "c" becomes "k".
    private static let sounds: [([Unicode.Scalar], [Unicode.Scalar])] = [
        ("tsch", "ch"), ("tch", "ch"), ("sch", "sh"), ("ph", "f"), ("ck", "k"), ("ch", "\u{1}"), ("c", "k"),
        ("\u{1}", "ch"), ("q", "k"), ("w", "v"), ("z", "j"), ("kh", "k"), ("gh", "g"), ("th", "t"),
        ("dh", "d"),
        ("bh", "b"), ("jh", "j"), ("ue", "u"), ("oe", "o"), ("ee", "i"), ("oo", "u"), ("ie", "i"),
        ("ai", "e"),
        ("ei", "e"), ("ay", "e"), ("ey", "e"), ("y", "i"),
    ].map { (Array($0.unicodeScalars), Array($1.unicodeScalars)) }

    /// Every `from` in `a` as `to`, left to right, as Kotlin's `replace`.
    private static func replace(_ a: [Unicode.Scalar], _ from: [Unicode.Scalar], _ to: [Unicode.Scalar])
        -> [Unicode.Scalar]
    {
        guard a.count >= from.count, a.contains(from[0]) else { return a }
        var out: [Unicode.Scalar] = []
        out.reserveCapacity(a.count)
        var i = 0
        while i < a.count {
            if i + from.count <= a.count && a[i..<i + from.count].elementsEqual(from) {
                out += to
                i += from.count
            } else {
                out.append(a[i])
                i += 1
            }
        }
        return out
    }

    /// Letters that do not decompose into a base letter and an accent.
    private static func spell(_ s: String) -> String {
        guard s.unicodeScalars.contains(where: { $0.value >= 0xdf }) else { return s }
        var b = ""
        for c in s.unicodeScalars {
            switch c {
            case "ß": b += "ss"
            case "æ": b += "ae"
            case "œ": b += "oe"
            case "ø": b += "o"
            case "đ", "ð": b += "d"
            case "ł": b += "l"
            case "þ": b += "th"
            case "ı": b += "i"
            default: b.unicodeScalars.append(c)
            }
        }
        return b
    }

    // Devanagari, U+0915 to U+0939.
    private static let consonants = [
        "k", "kh", "g", "gh", "n", "ch", "chh", "j", "jh", "n", "t", "th", "d", "dh", "n", "t", "th", "d",
        "dh", "n",
        "n", "p", "ph", "b", "bh", "m", "y", "r", "r", "l", "l", "l", "v", "sh", "sh", "s", "h",
    ]

    // The consonants with a nukta, U+0958 to U+095F, and what a separate nukta does to a consonant.
    private static let nuktaForms = ["q", "kh", "g", "z", "d", "dh", "f", "y"]
    private static let nukta = ["k": "q", "j": "z", "ph": "f"]

    // Vowel signs, U+093E to U+094C, and vowels, U+0904 to U+0914.
    private static let signs = [
        "aa", "i", "ii", "u", "uu", "ri", "ri", "e", "e", "e", "ai", "o", "o", "o", "au",
    ]
    private static let vowels = [
        "a", "a", "aa", "i", "ii", "u", "uu", "ri", "l", "e", "e", "e", "ai", "o", "o", "o", "au",
    ]

    /// A consonant and its vowel: "a" until a sign, a virama ("") or the dropping of a final a says otherwise.
    private final class Letter {
        var latin: String
        var vowel = "a"
        var open = true
        init(_ latin: String) { self.latin = latin }
    }

    private enum Piece {
        case letter(Letter)
        /// The Latin of a vowel or sign.
        case text(String)
    }

    /// Devanagari in Latin letters. The inherent a is dropped where Hindi
    /// drops it: at the end of a word, and between a vowel and a consonant
    /// that has one (धड़कन is dhadkan). Worked from the right, so each letter
    /// sees what was decided for the next.
    public static func devanagari(_ s: String) -> String {
        guard s.unicodeScalars.contains(where: { (0x900...0x97f).contains($0.value) }) else { return s }
        var out = ""
        var word: [Piece] = []
        func flush() {
            for i in word.indices.reversed() {
                guard case .letter(let l) = word[i], l.open else { continue }
                let next = i + 1 < word.count ? word[i + 1] : nil
                let prev = i > 0 ? word[i - 1] : nil
                let voweled: Bool
                switch prev {
                case .text: voweled = true
                case .letter(let p): voweled = !p.vowel.isEmpty
                case nil: voweled = false
                }
                var nextVoweled = false
                if case .letter(let n) = next { nextVoweled = !n.vowel.isEmpty }
                if i > 0 && (next == nil || (nextVoweled && voweled)) { l.vowel = "" }
            }
            for p in word {
                switch p {
                case .letter(let l): out += l.latin + l.vowel
                case .text(let t): out += t
                }
            }
            word.removeAll()
        }
        for c in s.unicodeScalars {
            var last: Letter?
            if case .letter(let l) = word.last { last = l }
            let v = Int(c.value)
            switch v {
            case 0x915...0x939: word.append(.letter(Letter(consonants[v - 0x915])))
            case 0x958...0x95f: word.append(.letter(Letter(nuktaForms[v - 0x958])))
            case 0x93c: if let last { last.latin = nukta[last.latin] ?? last.latin }
            case 0x93e...0x94c:
                let sign = signs[v - 0x93e]
                if let last, last.open {
                    last.vowel = sign
                    last.open = false
                } else {
                    word.append(.text(sign))
                }
            case 0x94d:
                last?.vowel = ""
                last?.open = false
            case 0x901, 0x902, 0x903:
                // A nasal or visarga keeps the a before it: हंस is hans.
                last?.open = false
                word.append(.text(v == 0x903 ? "h" : "n"))
            case 0x904...0x914: word.append(.text(vowels[v - 0x904]))
            case 0x960: word.append(.text("ri"))
            case 0x961: word.append(.text("l"))
            case 0x93d: break
            case 0x950:
                flush()
                out += "om"
            case 0x966...0x96f:
                flush()
                out += String(v - 0x966)
            default:
                flush()
                if v == 0x964 || v == 0x965 { out += " " } else { out.unicodeScalars.append(c) }
            }
        }
        flush()
        return out
    }
}

/// How well one query word matches one word; lower is better, `noMatch` is not at all.
let noMatch = Int.max

/// What was typed: words, each perhaps narrowed to a field with `artist:`,
/// `album:` and the like; `year:2010-2015` is a range.
public struct SearchQuery: Sendable {
    public struct Term: Sendable {
        public let word: String
        public let field: String?
        public let years: ClosedRange<Int>?
        // As bytes, compared with memcmp and memmem: Swift's own string search
        // made a keystroke over 7,000 songs take a fifth of a second.
        let bytes: [UInt8]
        let key: [UInt8]
        /// The key as the phone counts letters, for typos.
        private let keyUnits: [UInt16]

        init(_ word: String, _ field: String? = nil, _ years: ClosedRange<Int>? = nil) {
            self.word = word
            self.field = field
            self.years = years
            bytes = Array(word.utf8)
            let k = Fold.key(word)
            key = Array(k.utf8)
            keyUnits = Array(k.utf16)
        }

        func quality(_ w: [UInt8], _ k: [UInt8]) -> Int {
            if w == bytes { return 0 }
            if starts(w, bytes) { return 1 }
            if has(w, bytes) { return 2 }
            if !key.isEmpty && starts(k, key) { return 3 }
            return noMatch
        }

        /// A typo away from the start of `k`, by sound: one edit in 4 to 7 letters, two from 8.
        /// `row` and `next` are scratch space, kept between calls.
        func typo(_ k: [UInt16], _ row: inout [Int], _ next: inout [Int]) -> Bool {
            let m = keyUnits.count
            if m < 4 { return false }
            let most = m >= 8 ? 2 : 1
            if k.count < m - most { return false }
            if row.count < k.count + 1 {
                row = [Int](repeating: 0, count: k.count + 1)
                next = row
            }
            for j in 0...k.count { row[j] = j }
            for i in 1...m {
                next[0] = i
                var low = i
                for j in stride(from: 1, through: k.count, by: 1) {
                    next[j] = min(
                        row[j] + 1, next[j - 1] + 1, row[j - 1] + (keyUnits[i - 1] == k[j - 1] ? 0 : 1))
                    low = min(low, next[j])
                }
                if low > most { return false }
                swap(&row, &next)
            }
            return row[0...k.count].min()! <= most
        }
    }

    public let terms: [Term]
    /// The words with no field, as one string: a title that is all of them comes first.
    public let whole: String
    /// What to look for in lyrics: the `lyrics:` words, or else every word if nothing is narrowed to another field.
    public let lyrics: String
    public var isEmpty: Bool { terms.isEmpty }

    public static let fields: Set<String> = [
        "title", "artist", "album", "composer", "genre", "year", "folder", "lyrics",
    ]

    public init(_ text: String) {
        var terms: [Term] = []
        for m in text.matches(of: token) {
            let field = m.1.map { $0.lowercased() }.flatMap { Self.fields.contains($0) ? $0 : nil }
            let value = unquote(String(field != nil ? m.2! : m.0))
            if field == "year", let r = value.wholeMatch(of: years), let a = Int(r.1), let b = Int(r.2) {
                terms.append(Term(value, field, min(a, b)...max(a, b)))
            } else {
                terms += Fold.words(value).map { Term($0, field) }
            }
        }
        self.terms = terms
        whole = terms.filter { $0.field == nil }.map(\.word).joined(separator: " ")
        let narrowed = terms.filter { $0.field == "lyrics" }
        lyrics =
            !narrowed.isEmpty
            ? narrowed.map(\.word).joined(separator: " ") : terms.contains { $0.field != nil } ? "" : whole
    }

    /// Whether a line of lyrics holds every word, by the start of a word or its sound; no typos among thousands of lines.
    public func inLine(_ words: [String]) -> Bool {
        let looked = terms.filter { $0.field == nil || $0.field == "lyrics" }
        if looked.isEmpty { return false }
        let w = words.map { Array($0.utf8) }
        let keys = words.map { Array(Fold.key($0).utf8) }
        return looked.allSatisfy { t in w.indices.contains { t.quality(w[$0], keys[$0]) <= 3 } }
    }
}

nonisolated(unsafe) private let token = /(\p{L}+):("[^"]*"|\S+)|"[^"]*"|\S+/
nonisolated(unsafe) private let years = /(\d{4})(?:-|\.\.)(\d{4})/

private func starts(_ a: [UInt8], _ b: [UInt8]) -> Bool { a.count >= b.count && memcmp(a, b, b.count) == 0 }
private func has(_ a: [UInt8], _ b: [UInt8]) -> Bool {
    a.count >= b.count && memmem(a, a.count, b, b.count) != nil
}

/// Kotlin's `removeSurrounding("\"")`.
private func unquote(_ s: String) -> String {
    s.count >= 2 && s.hasPrefix("\"") && s.hasSuffix("\"") ? String(s.dropFirst().dropLast()) : s
}

/// Something searched for in an item. `name` is what a filter calls it
/// (`artist:`). A lower `weight` ranks a match higher; with none the field
/// is searched only through its filter.
public struct SearchField<T> {
    let name: String
    let weight: Int?
    let text: (T) -> String

    public init(_ name: String, _ weight: Int?, _ text: @escaping (T) -> String) {
        self.name = name
        self.weight = weight
        self.text = text
    }
}

/// The words of every item, each distinct word folded once. A query is
/// compared with the distinct words, then each item is a few lookups, which
/// keeps a keystroke over 7,000 songs quick.
public final class SearchIndex<T: Sendable>: Sendable {
    public let items: [T]
    private let names: [String]
    private let weights: [Int?]
    private let words: [[UInt8]]
    private let keys: [[UInt8]]
    private let keyUnits: [[UInt16]]
    /// Words in a field searched without a filter: only they decide that a query word needs no typo match.
    private let open: [Bool]
    /// For each item and field, its words' numbers; its text as folded, for years and the title;
    /// and that text run together, for "acdc".
    private let cells: [[[Int]]]
    private let texts: [[String]]
    private let runs: [[[UInt8]]]

    public init(_ items: [T], _ fields: [SearchField<T>]) {
        self.items = items
        names = fields.map(\.name)
        weights = fields.map(\.weight)
        var ids: [String: Int] = [:]
        var words: [String] = []
        var keys: [String] = []
        let texts = items.map { item in fields.map { Fold.text($0.text(item)) } }
        cells = texts.map { item in
            item.map { t in
                t.isEmpty
                    ? []
                    : t.split(separator: " ").map { s in
                        let w = String(s)
                        if let id = ids[w] { return id }
                        words.append(w)
                        keys.append(Fold.key(w))
                        ids[w] = words.count - 1
                        return words.count - 1
                    }
            }
        }
        var open = [Bool](repeating: false, count: words.count)
        for item in cells {
            for (f, cell) in item.enumerated() where fields[f].weight != nil {
                for id in cell { open[id] = true }
            }
        }
        self.words = words.map { Array($0.utf8) }
        self.keys = keys.map { Array($0.utf8) }
        keyUnits = keys.map { Array($0.utf16) }
        self.open = open
        self.texts = texts
        runs = texts.map { $0.map { Array($0.utf8.filter { $0 != 32 }) } }
    }

    /// What `q` matches, in the items' own order.
    public func filter(_ q: SearchQuery) -> [T] {
        if q.isEmpty { return items }
        let s = scores(q)
        return items.indices.filter { s[$0] != noMatch }.map { items[$0] }
    }

    /// What `q` matches with its score, best (lowest) first. Among equal
    /// scores a higher `boost` comes first: the user's favourites and most
    /// played, never above a better match.
    public func search(_ q: SearchQuery, boost: (T) -> Int = { _ in 0 }) -> [(item: T, score: Int)] {
        if q.isEmpty { return [] }
        let s = scores(q)
        var out: [(i: Int, score: Int, boost: Int)] = []
        for i in items.indices where s[i] != noMatch {
            let title = texts[i][0]
            let bonus = q.whole.isEmpty ? 0 : title == q.whole ? 20 : title.hasPrefix(q.whole) ? 10 : 0
            out.append((i, s[i] - bonus, boost(items[i])))
        }
        out.sort { ($0.score, -$0.boost, $0.i) < ($1.score, -$1.boost, $1.i) }
        return out.map { (items[$0.i], $0.score) }
    }

    private func scores(_ q: SearchQuery) -> [Int] {
        let qualities: [[Int]?] = q.terms.map { t in
            if t.years != nil { return nil }
            var a = words.indices.map { t.quality(words[$0], keys[$0]) }
            // Typos only for a word that matches nothing better anywhere: "love" must not bring "live".
            if !a.indices.contains(where: { a[$0] <= 3 && (open[$0] || t.field != nil) }) {
                var row: [Int] = []
                var next: [Int] = []
                for i in a.indices where t.typo(keyUnits[i], &row, &next) { a[i] = 4 }
            }
            return a
        }
        // Each term's weight in each field, or -1 where it is not looked for.
        let plan = q.terms.map { t in
            names.indices.map { f in
                if let field = t.field { return field == names[f] ? weights[f] ?? 0 : -1 }
                return weights[f] ?? -1
            }
        }
        return items.indices.map { i in
            var total = 0
            for (n, t) in q.terms.enumerated() {
                var best = noMatch
                for (f, w) in plan[n].enumerated() where w >= 0 {
                    if let ys = t.years {
                        if let y = Int(texts[i][f]), ys.contains(y) { best = min(best, w) }
                        continue
                    }
                    let a = qualities[n]!
                    for id in cells[i][f] where a[id] != noMatch { best = min(best, a[id] * 10 + w) }
                    // The field's words run together: "acdc" finds AC/DC.
                    if best > 20 + w && t.bytes.count >= 3 && has(runs[i][f], t.bytes) {
                        best = 20 + w
                    }
                }
                if best == noMatch { return noMatch }
                total += best
            }
            return total
        }
    }
}

extension SearchIndex where T == Song {
    /// A song's fields, most telling first (docs/plans/029_search.md).
    public static func songs(_ songs: [Song]) -> SearchIndex<Song> {
        SearchIndex(
            songs,
            [
                SearchField("title", 0) { $0.title },
                SearchField("artist", 1) { ($0.artists + [$0.artist]).joined(separator: " ") },
                SearchField("album", 2) { $0.album },
                SearchField("artist", 3) { $0.albumArtist },
                SearchField("composer", 3) { $0.composer },
                SearchField("genre", 4) { $0.genres.joined(separator: " ") },
                SearchField("year", 4) { $0.year > 0 ? String($0.year) : "" },
                SearchField("folder", nil) { $0.folder },
            ])
    }
}

extension SearchIndex {
    /// A list searched by its names alone, as artists, genres or playlists; `field` is its filter.
    public static func names(_ items: [T], _ field: String, _ name: @escaping (T) -> String) -> SearchIndex<T>
    {
        SearchIndex(items, [SearchField(field, 0, name)])
    }
}

/// A line of lyrics found: the song and the line, as the server answers.
public struct LyricsHit: Decodable, Sendable, Hashable {
    public let song: Int
    public let line: String
}

/// At most this many songs found in lyrics, here and on the server.
public let lyricsHits = 50

/// The first matching line of each song, songs whose line holds the words as typed first, as the server ranks them.
public func searchKept(_ rows: [(song: Int, text: String)], _ q: SearchQuery) -> [LyricsHit] {
    if q.lyrics.isEmpty { return [] }
    var hits: [(hit: LyricsHit, typed: Bool)] = []
    for r in rows {
        for l in parseLyrics(r.text).lines {
            let words = Fold.words(l.text)
            guard q.inLine(words) else { continue }
            hits.append(
                (
                    LyricsHit(song: r.song, line: l.text.trimmingCharacters(in: .whitespaces)),
                    words.joined(separator: " ").contains(q.lyrics)
                ))
            break
        }
    }
    return Array(
        hits.enumerated().sorted {
            ($0.element.typed ? 0 : 1, $0.offset) < ($1.element.typed ? 0 : 1, $1.offset)
        }
        .prefix(lyricsHits).map(\.element.hit))
}
