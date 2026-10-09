package io.github.vivekg7.dhun

import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.sameSong
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// docs/plans/026_without_an_account.md
class KeptDownloadsTest {
    private fun song(
        path: String,
        ms: Long,
    ) = Song(7, path, "Song", "", "", "", "", "", "", 0, 0, 0, ms, "mpeg", 0, 0, 0, 1000, false, false, 0, false)

    // A kept file is named by the song ID, and another server's ID 7 is another song:
    // kept, it would play the wrong song, or be overwritten by it.
    @Test
    fun anotherServersSongUnderTheSameIdIsNotTheKeptOne() {
        assertFalse(sameSong(song("A/1.mp3", 200_000), song("B/9.flac", 250_000)))
    }

    @Test
    fun theSameServerAtAnotherAddressKeepsItsSongs() {
        assertTrue(sameSong(song("A/1.mp3", 200_000), song("A/1.mp3", 200_000)))
        // Moved on the NAS: the same ID and length, a new path.
        assertTrue(sameSong(song("A/1.mp3", 200_000), song("Albums/A/1.mp3", 200_000)))
        // Upgraded to FLAC where it was: the same path; its length may differ by a frame.
        assertTrue(sameSong(song("A/1.mp3", 200_000), song("A/1.mp3", 201_500)))
    }
}
