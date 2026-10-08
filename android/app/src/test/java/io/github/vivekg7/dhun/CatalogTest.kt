package io.github.vivekg7.dhun

import io.github.vivekg7.dhun.data.Catalog
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.ui.serverUrl
import org.junit.Assert.assertEquals
import org.junit.Test

class CatalogTest {
    private fun song(
        id: Long,
        path: String,
        title: String,
        album: String = "",
        artist: String = "",
        albumArtist: String = "",
        track: Int = 0,
    ) = Song(id, path, title, artist, artist, album, albumArtist, "", "", 0, track, 0, 180_000, "mpeg", 0, 0, 0, 0, false, false, 0, false)

    @Test
    fun albumsWithTheSameNameByDifferentArtistsStaySeparate() {
        val c =
            Catalog(
                listOf(
                    song(1, "A/Greatest Hits/01.mp3", "One", "Greatest Hits", "Queen"),
                    song(2, "B/Greatest Hits/01.mp3", "Two", "Greatest Hits", "ABBA"),
                    song(3, "A/Greatest Hits/02.mp3", "Three", "Greatest Hits", "Queen"),
                ),
            )
        assertEquals(listOf(1, 2), c.albums.map { it.songs.size }.sorted())
    }

    @Test
    fun compilationTaggedWithAnAlbumArtistIsOneAlbumAcrossFolders() {
        val c =
            Catalog(
                listOf(
                    song(1, "x/01.mp3", "One", "Rockstar", "Mohit", albumArtist = "A.R. Rahman", track = 1),
                    song(2, "y/02.mp3", "Two", "Rockstar", "Javed", albumArtist = "A.R. Rahman", track = 2),
                ),
            )
        assertEquals(1, c.albums.size)
        assertEquals(listOf(1L, 2L), c.albums[0].songs.map { it.id })
    }

    @Test
    fun numbersSortAsNumbers() {
        val sorted = listOf("Track 10", "track 2", "Track 1").sortedWith(Catalog.TITLE)
        assertEquals(listOf("Track 1", "track 2", "Track 10"), sorted)
    }

    @Test
    fun searchIgnoresCaseAndAccents() {
        val c = Catalog(listOf(song(1, "a.mp3", "Halo", "I Am... Sasha Fierce", "Beyoncé"), song(2, "b.mp3", "Tum Hi Ho", "Aashiqui 2", "Arijit Singh")))
        assertEquals(listOf(1L), c.search("beyonce").map { it.id })
        assertEquals(listOf(2L), c.search("TUM hi").map { it.id })
    }

    @Test
    fun foldersHoldTheirSubfoldersSongs() {
        val c = Catalog(listOf(song(1, "Hindi/Arijit/01.mp3", "One"), song(2, "Hindi/02.mp3", "Two")))
        assertEquals(
            setOf(1L, 2L),
            c.folders
                .getValue("Hindi")
                .allSongs()
                .map { it.id }
                .toSet(),
        )
        assertEquals(
            listOf("Hindi"),
            c.folders
                .getValue("")
                .children
                .map { it.name },
        )
    }

    @Test
    fun serverAddressGetsSchemeAndDefaultPort() {
        assertEquals("http://my-nas:8585", serverUrl(" my-nas "))
        assertEquals("http://192.168.1.50:8585", serverUrl("192.168.1.50:8585/"))
        assertEquals("https://nas.example.ts.net", serverUrl("https://nas.example.ts.net"))
    }
}
