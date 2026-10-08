package io.github.vivekg7.dhun

import io.github.vivekg7.dhun.data.SongCache
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import java.io.File
import java.io.FileNotFoundException
import java.io.IOException
import kotlin.concurrent.thread

/** The player reading a song the cache is still fetching (docs/plans/019_networking_and_caching.md). */
class SongCacheTest {
    private fun entry() = SongCache.Entry(1, File("1.mp3.part"))

    // A reader that has caught up waits for the fetcher, not fails: that is
    // what lets a song play while it arrives.
    @Test
    fun aReaderWaitsForBytesStillComing() {
        val e = entry()
        e.written = 100
        thread {
            Thread.sleep(100)
            e.written = 200
            e.changed()
        }
        assertEquals(200, e.awaitPast(100))
    }

    @Test
    fun theEndOfAWholeFileIsTheEnd() {
        val e = entry()
        e.written = 100
        e.done = true
        assertEquals(100, e.awaitPast(100))
    }

    // An error, so the player retries and says it is waiting for the network;
    // a song the server no longer has is final, so the player moves on.
    @Test
    fun aFailingFetchIsAnErrorAndAGoneSongIsFinal() {
        val e = entry()
        e.written = 100
        e.failing = true
        assertThrows(IOException::class.java) { e.awaitPast(100) }
        assertEquals(100, e.awaitPast(50)) // what has arrived is still read
        e.gone = true
        assertThrows(FileNotFoundException::class.java) { e.awaitPast(100) }
    }
}
