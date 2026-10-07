package io.github.vivekg7.dhun

import io.github.vivekg7.dhun.data.LyricLine
import io.github.vivekg7.dhun.data.parseLyrics
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class LyricsTest {
    // Lines as the curation workflow's .lrc files have them: tags, a repeated
    // chorus on one line, two- and three-digit fractions, word stamps.
    @Test
    fun syncedLinesAreTimedSortedAndClean() {
        val p =
            parseLyrics(
                """
                [ar:Someone]
                [ti:Saans]
                [00:05.5]First
                [00:10.25][00:30.250]Chorus
                [00:20.00]<00:20.00>Word <00:21.50>stamps
                [00:25.00]
                """.trimIndent(),
            )
        assertEquals(
            listOf(
                LyricLine(5_500, "First"),
                LyricLine(10_250, "Chorus"),
                LyricLine(20_000, "Word stamps"),
                LyricLine(25_000, ""),
                LyricLine(30_250, "Chorus"),
            ),
            p.lines,
        )
        assertEquals(-1, p.at(5_000))
        assertEquals(1, p.at(19_999))
        assertEquals(4, p.at(99_000))
    }

    @Test
    fun offsetShowsLinesEarlier() {
        val p = parseLyrics("[offset:+500]\n[00:01.00]a\n[00:00.20]b")
        assertEquals(listOf(LyricLine(0, "b"), LyricLine(500, "a")), p.lines)
    }

    // Embedded lyrics are often plain; a stray tag must not make them "synced" or show up as a verse.
    @Test
    fun plainLyricsKeepTheirLinesButNotTags() {
        val p = parseLyrics("\n[ar:Someone]\nOne\n\nTwo\n\n")
        assertFalse(p.synced)
        assertEquals(listOf("One", "", "Two"), p.lines.map { it.text })
        assertEquals(-1, p.at(10_000))
    }
}
