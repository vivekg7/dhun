package io.github.vivekg7.dhun

import io.github.vivekg7.dhun.data.QueueRow
import io.github.vivekg7.dhun.play.Playback
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

class QueueOrderTest {
    private fun q(
        id: String,
        usedAt: Long,
    ) = QueueRow(id, id, "", 0, 0, false, "off", usedAt)

    // The order is a synced setting: it can name queues deleted elsewhere, miss
    // ones made on another device, or come from a newer app in another shape.
    @Test
    fun theOrderNeverLosesOrInventsAQueue() {
        val queues = listOf(q("a", 30), q("b", 10), q("c", 20), q("d", 5))
        val order = Json.parseToJsonElement("""["c", "gone", "a", "c", 7]""")
        // Named first, in order; the rest by use, oldest first, as Musicolet appends.
        assertEquals(listOf("c", "a", "d", "b"), Playback.orderOf(queues, order).map { it.id })
        assertEquals(listOf("d", "b", "c", "a"), Playback.orderOf(queues, null).map { it.id })
        assertEquals(listOf("d", "b", "c", "a"), Playback.orderOf(queues, JsonPrimitive("a")).map { it.id })
    }
}
