package io.github.vivekg7.dhun.data

import java.text.Normalizer
import kotlin.math.min

/**
 * Search, the same in every box (docs/plans/029_search.md). Text folds into
 * lower-case words, each word has a sound key that evens out romanised
 * spelling, and a query matches an item when every one of its words
 * matches a word of the item. The server folds lyrics the same way, and
 * api/search-fold.tsv keeps the two copies in step.
 */
object Fold {
    /** Lower-case words: Devanagari in Latin letters, accents off, "&" as "and", other punctuation splitting. */
    fun words(s: String): List<String> {
        val t = spell(Normalizer.normalize(devanagari(s), Normalizer.Form.NFKD).lowercase())
        val out = mutableListOf<String>()
        val w = StringBuilder()

        fun end() {
            if (w.isNotEmpty()) out += w.toString()
            w.clear()
        }
        for (c in t) {
            when {
                Character.getType(c).toByte() in MARKS -> {}

                c.isLetterOrDigit() -> {
                    w.append(c)
                }

                c in APOSTROPHES -> {}

                c == '&' -> {
                    end()
                    out += "and"
                }

                else -> {
                    end()
                }
            }
        }
        end()
        return out
    }

    /** The words of [s] joined by single spaces. */
    fun text(s: String) = words(s).joinToString(" ")

    /** A word's sound key: Kabhi and Kabhie, Main and Mein, Pyar and Pyaar come out the same. */
    fun key(word: String): String {
        var k = word
        for ((from, to) in SOUNDS) k = k.replace(from, to)
        val b = StringBuilder()
        for (c in k) if (b.isEmpty() || b.last() != c) b.append(c)
        if (b.length > 2 && b.last() == 'h' && b[b.length - 2] in "aeiou") b.setLength(b.length - 1)
        return b.toString()
    }

    private val MARKS = byteArrayOf(Character.NON_SPACING_MARK, Character.COMBINING_SPACING_MARK, Character.ENCLOSING_MARK)
    private const val APOSTROPHES = "'‘’ʼ`"

    /** In order. "ch" is set aside while a lone "c" becomes "k". */
    private val SOUNDS =
        listOf(
            "tsch" to "ch",
            "tch" to "ch",
            "sch" to "sh",
            "ph" to "f",
            "ck" to "k",
            "ch" to "\u0001",
            "c" to "k",
            "\u0001" to "ch",
            "q" to "k",
            "w" to "v",
            "z" to "j",
            "kh" to "k",
            "gh" to "g",
            "th" to "t",
            "dh" to "d",
            "bh" to "b",
            "jh" to "j",
            "ue" to "u",
            "oe" to "o",
            "ee" to "i",
            "oo" to "u",
            "ie" to "i",
            "ai" to "e",
            "ei" to "e",
            "ay" to "e",
            "ey" to "e",
            "y" to "i",
        )

    /** Letters that do not decompose into a base letter and an accent. */
    private fun spell(s: String): String {
        if (s.all { it.code < 0xdf }) return s
        val b = StringBuilder()
        for (c in s) {
            b.append(
                when (c) {
                    'ß' -> "ss"
                    'æ' -> "ae"
                    'œ' -> "oe"
                    'ø' -> "o"
                    'đ', 'ð' -> "d"
                    'ł' -> "l"
                    'þ' -> "th"
                    'ı' -> "i"
                    else -> c
                },
            )
        }
        return b.toString()
    }

    // Devanagari, U+0915 to U+0939.
    private val CONSONANTS =
        listOf(
            "k",
            "kh",
            "g",
            "gh",
            "n",
            "ch",
            "chh",
            "j",
            "jh",
            "n",
            "t",
            "th",
            "d",
            "dh",
            "n",
            "t",
            "th",
            "d",
            "dh",
            "n",
            "n",
            "p",
            "ph",
            "b",
            "bh",
            "m",
            "y",
            "r",
            "r",
            "l",
            "l",
            "l",
            "v",
            "sh",
            "sh",
            "s",
            "h",
        )

    // The consonants with a nukta, U+0958 to U+095F, and what a separate nukta does to a consonant.
    private val NUKTA_FORMS = listOf("q", "kh", "g", "z", "d", "dh", "f", "y")
    private val NUKTA = mapOf("k" to "q", "j" to "z", "ph" to "f")

    // Vowel signs, U+093E to U+094C, and vowels, U+0904 to U+0914.
    private val SIGNS = listOf("aa", "i", "ii", "u", "uu", "ri", "ri", "e", "e", "e", "ai", "o", "o", "o", "au")
    private val VOWELS = listOf("a", "a", "aa", "i", "ii", "u", "uu", "ri", "l", "e", "e", "e", "ai", "o", "o", "o", "au")

    /** A consonant and its vowel: "a" until a sign, a virama ("") or the dropping of a final a says otherwise. */
    private class Letter(
        var latin: String,
        var vowel: String = "a",
        var open: Boolean = true,
    )

    /**
     * Devanagari in Latin letters. The inherent a is dropped where Hindi
     * drops it: at the end of a word, and between a vowel and a consonant
     * that has one (धड़कन is dhadkan). Worked from the right, so each letter
     * sees what was decided for the next.
     */
    fun devanagari(s: String): String {
        if (s.none { it in 'ऀ'..'ॿ' }) return s
        val out = StringBuilder()
        val word = mutableListOf<Any>() // Letter, or the Latin of a vowel or sign

        fun flush() {
            for (i in word.indices.reversed()) {
                val l = word[i] as? Letter ?: continue
                if (!l.open) continue
                val next = word.getOrNull(i + 1)
                val prev = word.getOrNull(i - 1)
                val voweled = prev is String || (prev is Letter && prev.vowel.isNotEmpty())
                if (i > 0 && (next == null || (next is Letter && next.vowel.isNotEmpty() && voweled))) l.vowel = ""
            }
            for (x in word) out.append(if (x is Letter) x.latin + x.vowel else x)
            word.clear()
        }
        for (c in s) {
            val last = word.lastOrNull() as? Letter
            when (c) {
                in 'क'..'ह' -> {
                    word += Letter(CONSONANTS[c - 'क'])
                }

                in 'क़'..'य़' -> {
                    word += Letter(NUKTA_FORMS[c - 'क़'])
                }

                '़' -> {
                    if (last != null) last.latin = NUKTA[last.latin] ?: last.latin
                }

                in 'ा'..'ौ' -> {
                    val v = SIGNS[c - 'ा']
                    if (last != null && last.open) {
                        last.vowel = v
                        last.open = false
                    } else {
                        word += v
                    }
                }

                '्' -> {
                    if (last != null) {
                        last.vowel = ""
                        last.open = false
                    }
                }

                'ँ', 'ं', 'ः' -> {
                    // A nasal or visarga keeps the a before it: हंस is hans.
                    last?.open = false
                    word += if (c == 'ः') "h" else "n"
                }

                in 'ऄ'..'औ' -> {
                    word += VOWELS[c - 'ऄ']
                }

                'ॠ' -> {
                    word += "ri"
                }

                'ॡ' -> {
                    word += "l"
                }

                'ऽ' -> {}

                'ॐ' -> {
                    flush()
                    out.append("om")
                }

                in '०'..'९' -> {
                    flush()
                    out.append('0' + (c - '०'))
                }

                else -> {
                    flush()
                    out.append(if (c == '।' || c == '॥') ' ' else c)
                }
            }
        }
        flush()
        return out.toString()
    }
}

/** How well one query word matches one word; lower is better, [NONE] is not at all. */
internal const val NONE = Int.MAX_VALUE

/**
 * What was typed: words, each perhaps narrowed to a field with
 * `artist:`, `album:` and the like; `year:2010-2015` is a range.
 */
class SearchQuery(
    text: String,
) {
    class Term(
        val word: String,
        val field: String? = null,
        val years: IntRange? = null,
    ) {
        val key = Fold.key(word)

        fun quality(
            w: String,
            k: String,
        ) = when {
            w == word -> 0
            w.startsWith(word) -> 1
            w.contains(word) -> 2
            key.isNotEmpty() && k.startsWith(key) -> 3
            else -> NONE
        }

        /** A typo away from the start of [k], by sound: one edit in 4 to 7 letters, two from 8. */
        fun typo(k: String): Boolean {
            val m = key.length
            if (m < 4) return false
            val most = if (m >= 8) 2 else 1
            if (k.length < m - most) return false
            var row = IntArray(k.length + 1) { it }
            for (i in 1..m) {
                val next = IntArray(k.length + 1)
                next[0] = i
                var low = i
                for (j in 1..k.length) {
                    next[j] = minOf(row[j] + 1, next[j - 1] + 1, row[j - 1] + if (key[i - 1] == k[j - 1]) 0 else 1)
                    low = min(low, next[j])
                }
                if (low > most) return false
                row = next
            }
            return row.min() <= most
        }
    }

    val terms: List<Term>

    /** The words with no field, as one string: a title that is all of them comes first. */
    val whole: String

    /** What to look for in lyrics: the `lyrics:` words, or else every word if nothing is narrowed to another field. */
    val lyrics: String

    val isEmpty get() = terms.isEmpty()

    init {
        val terms = mutableListOf<Term>()
        for (m in TOKEN.findAll(text)) {
            val field = m.groupValues[1].lowercase().takeIf { it in FIELDS }
            val value = if (field != null) m.groupValues[2].removeSurrounding("\"") else m.value.removeSurrounding("\"")
            val range = if (field == "year") YEARS.matchEntire(value) else null
            if (range != null) {
                val (a, b) = range.destructured
                terms += Term(value, field, minOf(a.toInt(), b.toInt())..maxOf(a.toInt(), b.toInt()))
            } else {
                for (w in Fold.words(value)) terms += Term(w, field)
            }
        }
        this.terms = terms
        whole = terms.filter { it.field == null }.joinToString(" ") { it.word }
        val narrowed = terms.filter { it.field == "lyrics" }
        lyrics =
            when {
                narrowed.isNotEmpty() -> narrowed.joinToString(" ") { it.word }
                terms.any { it.field != null } -> ""
                else -> whole
            }
    }

    /** Whether a line of lyrics holds every word, by the start of a word or its sound; no typos among thousands of lines. */
    fun inLine(words: List<String>): Boolean {
        val looked = terms.filter { it.field == null || it.field == "lyrics" }
        if (looked.isEmpty()) return false
        val keys = words.map { Fold.key(it) }
        return looked.all { t -> words.indices.any { t.quality(words[it], keys[it]) <= 3 } }
    }

    companion object {
        val FIELDS = setOf("title", "artist", "album", "composer", "genre", "year", "folder", "lyrics")
        private val TOKEN = Regex("""(\p{L}+):("[^"]*"|\S+)|"[^"]*"|\S+""")
        private val YEARS = Regex("""(\d{4})(?:-|\.\.)(\d{4})""")
    }
}

/**
 * Something searched for in an item. [name] is what a filter calls it
 * (`artist:`). A lower [weight] ranks a match higher; with none the field
 * is searched only through its filter.
 */
class Field<T>(
    val name: String,
    val weight: Int?,
    val text: (T) -> String,
)

/**
 * The words of every item, each distinct word folded once. A query is
 * compared with the distinct words, then each item is a few lookups,
 * which keeps a keystroke over 7,000 songs quick.
 */
class SearchIndex<T>(
    val items: List<T>,
    private val fields: List<Field<T>>,
) {
    private val words = ArrayList<String>()
    private val keys = ArrayList<String>()

    /** Words in a field searched without a filter: only they decide that a query word needs no typo match. */
    private val open: BooleanArray

    /** For each item and field, its words' numbers; and its text as folded, for "acdc" and years. */
    private val cells: Array<Array<IntArray>>
    private val texts: Array<Array<String>>

    init {
        val ids = HashMap<String, Int>()
        texts = Array(items.size) { i -> Array(fields.size) { f -> Fold.text(fields[f].text(items[i])) } }
        cells =
            Array(items.size) { i ->
                Array(fields.size) { f ->
                    val t = texts[i][f]
                    if (t.isEmpty()) {
                        IntArray(0)
                    } else {
                        t
                            .split(' ')
                            .map { w ->
                                ids.getOrPut(w) {
                                    words += w
                                    keys += Fold.key(w)
                                    words.size - 1
                                }
                            }.toIntArray()
                    }
                }
            }
        open = BooleanArray(words.size)
        for (item in cells) for ((f, ids) in item.withIndex()) if (fields[f].weight != null) for (id in ids) open[id] = true
    }

    /** What [q] matches, in the items' own order, with each one's place in [items]. */
    fun filter(q: SearchQuery): List<IndexedValue<T>> {
        if (q.isEmpty) return items.withIndex().toList()
        val s = scores(q)
        return items.withIndex().filter { s[it.index] != NONE }
    }

    /**
     * What [q] matches with its score, best (lowest) first. Among equal
     * scores a higher [boost] comes first: the user's favourites and most
     * played, never above a better match.
     */
    fun search(
        q: SearchQuery,
        boost: (T) -> Int = { 0 },
    ): List<Pair<T, Int>> {
        if (q.isEmpty) return emptyList()
        val s = scores(q)
        val out = ArrayList<Triple<T, Int, Int>>()
        for (i in items.indices) {
            if (s[i] == NONE) continue
            val title = texts[i][0]
            val bonus =
                when {
                    q.whole.isEmpty() -> 0
                    title == q.whole -> 20
                    title.startsWith(q.whole) -> 10
                    else -> 0
                }
            out += Triple(items[i], s[i] - bonus, boost(items[i]))
        }
        out.sortWith(compareBy<Triple<T, Int, Int>> { it.second }.thenByDescending { it.third })
        return out.map { it.first to it.second }
    }

    private fun scores(q: SearchQuery): IntArray {
        val qualities =
            q.terms.map { t ->
                if (t.years != null) return@map null
                val a = IntArray(words.size) { t.quality(words[it], keys[it]) }
                // Typos only for a word that matches nothing better anywhere: "love" must not bring "live".
                if (a.indices.none { a[it] <= 3 && (open[it] || t.field != null) }) for (i in a.indices) if (t.typo(keys[i])) a[i] = 4
                a
            }
        return IntArray(items.size) { i ->
            var total = 0
            for ((n, t) in q.terms.withIndex()) {
                var best = NONE
                for ((f, field) in fields.withIndex()) {
                    val w = field.weight ?: if (t.field == null) continue else 0
                    if (t.field != null && t.field != field.name) continue
                    if (t.years != null) {
                        if (texts[i][f].toIntOrNull()?.let { it in t.years } == true) best = min(best, w)
                        continue
                    }
                    val a = qualities[n]!!
                    for (id in cells[i][f]) if (a[id] != NONE) best = min(best, a[id] * 10 + w)
                    // The field's words run together: "acdc" finds AC/DC.
                    if (best > 20 + w && t.word.length >= 3 && texts[i][f].replace(" ", "").contains(t.word)) best = 20 + w
                }
                if (best == NONE) return@IntArray NONE
                total += best
            }
            total
        }
    }

    companion object {
        /** A song's fields, most telling first (docs/plans/029_search.md). */
        val SONG: List<Field<Song>> =
            listOf(
                Field("title", 0) { it.title },
                Field("artist", 1) { (it.artistList + it.artist).joinToString(" ") },
                Field("album", 2) { it.album },
                Field("artist", 3) { it.albumArtist },
                Field("composer", 3) { it.composer },
                Field("genre", 4) { it.genreList.joinToString(" ") },
                Field("year", 4) { if (it.year > 0) it.year.toString() else "" },
                Field("folder", null) { it.folder },
            )

        fun songs(songs: List<Song>) = SearchIndex(songs, SONG)

        /** A list searched by its names alone, as artists, genres or playlists; [field] is its filter. */
        fun <T> names(
            items: List<T>,
            field: String,
            name: (T) -> String,
        ) = SearchIndex(items, listOf(Field(field, 0, name)))
    }
}
