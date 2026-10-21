package io.github.vivekg7.dhun

import io.github.vivekg7.dhun.data.SongCache
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.io.FileNotFoundException
import java.io.IOException
import java.io.RandomAccessFile
import kotlin.concurrent.thread

/** The player reading a song the cache is still fetching (docs/plans/019_networking_and_caching.md). */
class SongCacheTest {
    private fun entry() = SongCache.Entry(1, File("1.mp3.part")).also { it.add(0, 0) }

    private fun SongCache.Entry.written(n: Long) = add(0, n)

    private val mb = 1L shl 20

    // A reader that has caught up waits for the fetcher, not fails: that is
    // what lets a song play while it arrives.
    @Test
    fun aReaderWaitsForBytesStillComing() {
        val e = entry()
        e.written(100)
        thread {
            Thread.sleep(100)
            e.written(200)
        }
        assertEquals(200, e.awaitPast(100))
    }

    @Test
    fun theEndOfAWholeFileIsTheEnd() {
        val e = entry()
        e.written(100)
        e.done = true
        assertEquals(100, e.awaitPast(100))
    }

    // An error, so the player retries and says it is waiting for the network;
    // a song the server no longer has is final, so the player moves on.
    @Test
    fun aFailingFetchIsAnErrorAndAGoneSongIsFinal() {
        val e = entry()
        e.written(100)
        e.failing = true
        assertThrows(IOException::class.java) { e.awaitPast(100) }
        assertEquals(100, e.awaitPast(50)) // what has arrived is still read
        e.gone = true
        assertThrows(FileNotFoundException::class.java) { e.awaitPast(100) }
    }

    // A long file is fetched around the reader (docs/plans/037_long_file_cache_window.md):
    // runs arrive out of order and merge, the fetcher stops a window ahead
    // of the reader, goes on as the reader moves, and jumps to a reader
    // waiting in a hole.
    @Test
    fun aWindowFollowsTheReader() {
        val e = SongCache.Entry(1, File("1.m4b.window"), ahead = mb, behind = mb)
        e.total = 10 * mb
        e.readAt = 0
        assertEquals(0L, e.next())
        assertEquals(mb, e.goal(0))
        e.add(mb / 2, mb)
        e.add(0, mb / 2)
        assertEquals(mb, e.arrived(0))
        assertNull(e.next()) // the window is full
        e.readAt = mb / 2
        assertEquals(mb, e.next())
        assertEquals(mb + mb / 2, e.goal(mb))
        val reader =
            thread {
                assertEquals(8 * mb + 1, e.awaitPast(8 * mb))
            }
        while (e.waitingAt < 0) Thread.sleep(5)
        assertEquals(8 * mb, e.next())
        assertTrue(e.goal(8 * mb) >= 8 * mb + SongCache.Entry.MIN_FETCH)
        e.add(8 * mb, 8 * mb + 1)
        reader.join()
        assertEquals(-1L, e.waitingAt)
    }

    // The roll keeps the bytes behind the reader in a new file at their
    // offsets, drops the rest, and tells a reader to reopen.
    @Test
    fun aRollKeepsTheBehindWindow() {
        val file = File.createTempFile("dhun", ".window")
        try {
            val e = SongCache.Entry(1, file, ahead = 4 * mb, behind = 256 * 1024)
            e.total = 100 * mb
            val bytes = ByteArray(mb.toInt()) { (it % 251).toByte() }
            RandomAccessFile(file, "rw").use { it.write(bytes) }
            e.add(0, mb)
            e.readAt = 768 * 1024
            assertTrue(e.rollDue())
            e.roll()
            assertEquals(1, e.generation)
            assertEquals(300L * 1024, e.arrived(300 * 1024)) // dropped
            assertEquals(mb, e.arrived(600 * 1024)) // kept, from 256 KB before the reader
            RandomAccessFile(e.file, "r").use {
                it.seek(700 * 1024)
                assertEquals(bytes[700 * 1024], it.readByte())
            }
            e.readAt = 900 * 1024
            assertTrue(!e.rollDue())
        } finally {
            file.delete()
        }
    }
}
