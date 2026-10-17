package io.github.vivekg7.dhun

import io.github.vivekg7.dhun.data.Catalog
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.play.Playback
import io.github.vivekg7.dhun.ui.Heard
import io.github.vivekg7.dhun.ui.day
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Podcasts and audiobooks (docs/plans/031_podcasts_and_audiobooks.md). */
class SpokenTest {
    private fun song(
        id: Long,
        path: String,
        kind: String = "",
        group: String = "",
        index: Int = 0,
    ) = Song(
        id,
        path,
        path.substringAfterLast('/'),
        "Host",
        "Host",
        "Show",
        "",
        "",
        "Podcast",
        0,
        0,
        0,
        600_000,
        "opus",
        0,
        0,
        0,
        0,
        false,
        false,
        0,
        false,
        kind = kind,
        groupKey = group,
        groupTitle = group.uppercase(),
        groupIndex = index,
    )

    // Each collection's paths are relative to its own folder, so an episode's
    // folder can be named like a music folder; it must still stay out of
    // every music list.
    @Test
    fun episodesStayOutOfMusicButQueuesFindThem() {
        val c =
            Catalog(
                listOf(
                    song(1, "Lex/song.mp3"),
                    song(2, "Lex/002.opus", Song.PODCAST, "lex", 1),
                    song(3, "Lex/001.opus", Song.PODCAST, "lex", 0),
                ),
            )
        assertEquals(listOf(1L), c.songs.map { it.id })
        assertEquals(
            listOf(1L),
            c.folders
                .getValue("Lex")
                .songs
                .map { it.id },
        )
        assertTrue(c.artists.all { g -> g.songs.none { it.spoken } } && c.genres.all { g -> g.songs.none { it.spoken } })
        assertEquals(
            listOf(3L, 2L),
            c.shows
                .single()
                .items
                .map { it.id },
        )
        assertEquals("LEX", c.shelfOf(c.byId.getValue(2))?.title)
    }

    @Test
    fun aBookCarriesOnFromItsFirstFileNotPlayed() {
        val c = Catalog((1L..3L).map { song(it, "Dune/$it.mp3", Song.AUDIOBOOK, "dune", it.toInt()) })
        val book = c.books.single()
        val heard = Heard(played = setOf(1L), left = mapOf(2L to 300_000L))
        assertEquals(2L, heard.next(book).id)
        assertEquals("50% · 15 min left", heard.progress(book))
        assertEquals(1L, Heard(setOf(1L, 2L, 3L), emptyMap()).next(book).id)
    }

    // Some podcast tools write the date tag without dashes.
    @Test
    fun aCompactDateReadsAsADay() {
        val s = song(1, "Show/1.mp3", Song.PODCAST, "show")
        assertEquals("2025-12-30", s.copy(date = "20251230").day)
        assertEquals("2025-12-30", s.copy(date = "2025-12-30T08:00:00Z").day)
        assertEquals("2025", s.copy(date = "2025").day)
    }

    /** Stopped mid-sentence in an episode: going on later starts the sentence again, a short pause does not. */
    @Test
    fun aLongPauseGoesBackALittle() {
        val minute = 60_000L
        assertEquals(80_000L, Playback.rewound(90_000, 6 * minute, 10_000, 5 * minute))
        assertEquals("a moment's pause", 90_000L, Playback.rewound(90_000, 30_000, 10_000, 5 * minute))
        assertEquals("never before the start", 0L, Playback.rewound(4_000, 6 * minute, 10_000, 5 * minute))
        assertEquals("turned off", 90_000L, Playback.rewound(90_000, 6 * minute, 0, 5 * minute))
    }
}
