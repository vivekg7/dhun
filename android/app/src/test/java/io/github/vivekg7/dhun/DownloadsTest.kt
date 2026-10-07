package io.github.vivekg7.dhun

import io.github.vivekg7.dhun.data.Catalog
import io.github.vivekg7.dhun.data.Downloads
import io.github.vivekg7.dhun.data.Mark
import io.github.vivekg7.dhun.data.Pin
import io.github.vivekg7.dhun.data.Playlist
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.Store
import io.github.vivekg7.dhun.data.covered
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class DownloadsTest {
    private fun song(
        id: Long,
        path: String,
    ) = Song(id, path, "Song $id", "", "", "", "", "", "", 0, 0, 0, 180_000, "mpeg", 0, 0, 0, 1000, false, false, 0, false)

    private val catalog =
        Catalog(
            listOf(
                song(1, "Hindi/90s/a.mp3"),
                song(2, "Hindi/90s/Live/b.mp3"),
                song(3, "Hindi/c.mp3"),
                song(4, "English/d.mp3"),
            ),
        )

    private fun pin(
        kind: String,
        ref: String,
        at: Long,
    ) = Pin(Downloads.key(kind, ref), kind, ref, ref, at)

    /** Before the catalogue loads, nothing is covered: an empty answer would delete every file. */
    @Test
    fun anEmptyCatalogueCoversNothingRatherThanNoSongs() {
        assertNull(covered(listOf(pin(Downloads.SONG, "1", 0)), Catalog(emptyList()), emptyList(), emptyList(), emptyList()))
    }

    /** A folder keeps its subfolders, a song in two pins is wanted once, and the newest pin comes first. */
    @Test
    fun pinsBecomeSongsOnceNewestFirst() {
        val pins =
            listOf(
                pin(Downloads.FOLDER, "Hindi/90s", at = 1),
                pin(Downloads.PLAYLIST, "7", at = 3),
                pin(Downloads.LIST, Store.FAV, at = 2),
            )
        val playlists = listOf(Playlist(7, "Mix", "", false, "4,2,0"))
        val favs = listOf(Mark(Store.FAV, 3, 0, false), Mark(Store.FAV, 1, 0, false))
        val ids = covered(pins, catalog, playlists, favs, emptyList())!!.map { it.id }
        assertEquals(listOf(4L, 2L, 3L, 1L), ids)
    }
}
