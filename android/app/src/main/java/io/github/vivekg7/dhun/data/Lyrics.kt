package io.github.vivekg7.dhun.data

import io.github.vivekg7.dhun.App
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

/**
 * Lyrics (docs/plans/015_lyrics.md), as the server stores them: a sibling
 * `.lrc` file or embedded tags. Every copy fetched is kept in Room, so the
 * lyrics of a downloaded song, or of one already viewed, show offline too.
 */
class Lyrics(
    private val app: App,
) {
    private val dao get() = app.db.dao()

    sealed interface State {
        data object Loading : State

        /** The server has none for this song. */
        data object None : State

        /** Not kept on the phone, and the server cannot be reached. */
        data object Offline : State

        data class Shown(
            val lyrics: Parsed,
        ) : State
    }

    /** The kept copy first, then the server's when it differs. */
    fun of(song: Song): Flow<State> =
        flow {
            if (!song.hasLyrics) return@flow emit(State.None)
            val kept = dao.lyrics(song.id)?.text
            emit(if (kept != null) State.Shown(parseLyrics(kept)) else State.Loading)
            try {
                val text = fetch(song.id)
                if (text == null) {
                    emit(State.None)
                } else if (text != kept) {
                    emit(State.Shown(parseLyrics(text)))
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                // Unreachable, or an answer we cannot read: the kept copy, if any, stays up.
                if (kept == null) emit(State.Offline)
            }
        }

    /** Fetches and keeps a song's lyrics; null (and forgotten) when the server has none. */
    suspend fun fetch(song: Long): String? {
        val text =
            try {
                app.api.lyrics(song).text
            } catch (e: ApiException) {
                if (e.code != 404) throw e
                dao.deleteLyrics(song)
                return null
            }
        dao.putLyrics(LyricsRow(song, text, System.currentTimeMillis()))
        return text
    }

    /** For downloads: the lyrics of [songs] not yet kept. Lyrics are small, so this runs on any network. */
    suspend fun keep(songs: List<Song>) {
        val kept = dao.lyricsSongs().toSet()
        for (s in songs) if (s.hasLyrics && s.id !in kept) fetch(s.id)
    }
}

/** One line; [ms] is where it starts, or -1 in lyrics without times. */
data class LyricLine(
    val ms: Long,
    val text: String,
)

data class Parsed(
    val synced: Boolean,
    val lines: List<LyricLine>,
) {
    /** The line playing at [ms]: the last one started, or -1 before the first. */
    fun at(ms: Long): Int {
        if (!synced) return -1
        var i = -1
        for ((j, l) in lines.withIndex()) if (l.ms <= ms) i = j else break
        return i
    }
}

private val STAMP = Regex("""\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?]""")
private val TAG = Regex("""^\[([a-zA-Z#]+):(.*)]$""")
private val WORD = Regex("""<\d{1,3}:\d{1,2}(?:[.:]\d{1,3})?>""")

/**
 * LRC by our own small parser (docs/plans/007_client_architecture.md):
 * `[mm:ss.xx]` stamps, several on one line for a repeated chorus,
 * `[offset:±ms]`, and word stamps (`<mm:ss.xx>`, enhanced LRC), which are
 * dropped. Text with no stamps is shown as plain lyrics, without its
 * `[ar:…]`-style tags.
 */
fun parseLyrics(text: String): Parsed {
    var offset = 0L
    val timed = mutableListOf<LyricLine>()
    val plain = mutableListOf<String>()
    for (raw in text.lineSequence()) {
        var line = raw.trim()
        val tag = TAG.matchEntire(line)
        if (tag != null && !STAMP.matches(line)) {
            if (tag.groupValues[1].equals("offset", true)) offset = tag.groupValues[2].trim().toLongOrNull() ?: 0
            continue
        }
        val times = mutableListOf<Long>()
        while (true) {
            val m = STAMP.matchAt(line, 0) ?: break
            val (min, sec, frac) = m.destructured
            times += min.toLong() * 60_000 + sec.toLong() * 1000 + frac.padEnd(3, '0').toLong()
            line = line.substring(m.range.last + 1)
        }
        line = line.replace(WORD, "").trim()
        if (times.isEmpty()) plain += line else times.forEach { timed += LyricLine(it, line) }
    }
    if (timed.isEmpty()) {
        // Blank lines at either end are file layout, not verses.
        return Parsed(false, plain.dropWhile { it.isEmpty() }.dropLastWhile { it.isEmpty() }.map { LyricLine(-1, it) })
    }
    // A positive offset shows the lines earlier.
    return Parsed(true, timed.map { it.copy(ms = (it.ms - offset).coerceAtLeast(0)) }.sortedBy { it.ms })
}
