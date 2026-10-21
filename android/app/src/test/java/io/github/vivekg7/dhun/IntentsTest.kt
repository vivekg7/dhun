package io.github.vivekg7.dhun

import android.provider.MediaStore
import io.github.vivekg7.dhun.data.Catalog
import io.github.vivekg7.dhun.data.Playlist
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.play.Intents
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** What "play X on Dhun" plays (docs/plans/038_voice_search_and_assistant.md). */
class IntentsTest {
    private fun song(
        id: Long,
        title: String,
        artist: String,
        album: String = "",
        kind: String = "",
        group: String = "",
        index: Int = 0,
    ) = Song(
        id,
        "a/$id.mp3",
        title,
        artist,
        artist,
        album,
        artist,
        "",
        "",
        0,
        0,
        0,
        180_000,
        "mpeg",
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
        groupIndex = index,
    )

    private val catalog =
        Catalog(
            listOf(
                song(1, "Tum Hi Ho", "Arijit Singh", "Aashiqui 2"),
                song(2, "Chahun Main Ya Naa", "Arijit Singh", "Aashiqui 2"),
                song(3, "Kabhie Kabhie", "Mukesh"),
                song(4, "Mukesh Ki Yaad", "Lata Mangeshkar"),
                song(5, "Chapter 1", "Author", kind = Song.AUDIOBOOK, group = "book", index = 1),
                song(6, "Chapter 2", "Author", kind = Song.AUDIOBOOK, group = "book", index = 2),
            ),
        )
    private val playlists = listOf(Playlist(9, "Morning drive", "", false, "3,1"))

    private fun ask(
        query: String,
        focus: String? = null,
        vararg hints: Pair<String, String>,
        played: Set<Long> = emptySet(),
    ) = Intents.resolve(catalog, playlists, played, query, focus) { k -> hints.firstOrNull { it.first == k }?.second }

    @Test
    fun anArtistNameBeatsTheSongsNamingHim() {
        // "Mukesh" is a song title too, by someone else; the artist section wins the tie.
        val pick = ask("mukesh")!!
        assertEquals("Mukesh", pick.name)
        assertEquals(listOf(3L), pick.songs.map { it.id })
    }

    @Test
    fun aTitlePlaysTheSongsFoundBestFirst() {
        val pick = ask("tum hi ho")!!
        assertEquals("search", pick.source)
        assertEquals(1L, pick.songs.first().id)
    }

    @Test
    fun theAssistantsHintNamesTheField() {
        val pick = ask("play aashiqui 2", MediaStore.Audio.Albums.ENTRY_CONTENT_TYPE, MediaStore.EXTRA_MEDIA_ALBUM to "Aashiqui 2")!!
        assertEquals("Aashiqui 2", pick.name)
        assertEquals(listOf(1L, 2L), pick.songs.map { it.id }.sorted())
        assertEquals("Morning drive", ask("morning drive")!!.name)
    }

    @Test
    fun aBookStartsAtItsFirstUnplayedChapter() {
        assertEquals(1, ask("author", played = setOf(5)).let { it!!.start })
    }

    @Test
    fun nothingMatchingPlaysNothing() {
        assertNull(ask("beethoven"))
        assertNull(ask(""))
    }
}
