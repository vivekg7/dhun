package io.github.vivekg7.dhun

import io.github.vivekg7.dhun.play.Playback.Companion.MAX_QUEUE_SONGS
import io.github.vivekg7.dhun.play.Playback.Companion.capQueue
import io.github.vivekg7.dhun.play.Playback.Companion.fitting
import org.junit.Assert.assertEquals
import org.junit.Test

class QueueCapTest {
    // The server trims by the same rule (plan 040); trimming differently here
    // would have it trim again, and the two copies of the queue differ.
    @Test
    fun aLongListWrapsRoundFromTheSongPlayedAndAFullQueueOnlyMoves() {
        val list = (1L..1500L).toList()
        val q = capQueue(list, 1201)
        assertEquals(MAX_QUEUE_SONGS, q.size)
        assertEquals((1201L..1500L) + (1L..700L), q)
        assertEquals(list.take(10), capQueue(list.take(10), 5))

        val full = (1L..999L).toList()
        // One new song fits; then only songs already queued, which move.
        assertEquals(listOf(5000L, 3L), fitting(full, listOf(5000, 5001, 3)))
        assertEquals(listOf(3L), fitting(full + 5000, listOf(5001, 3)))
    }
}
