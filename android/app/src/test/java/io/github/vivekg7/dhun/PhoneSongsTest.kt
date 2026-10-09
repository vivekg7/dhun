package io.github.vivekg7.dhun

import io.github.vivekg7.dhun.data.Store
import io.github.vivekg7.dhun.data.mergePhoneSongs
import io.github.vivekg7.dhun.data.phoneId
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// docs/plans/025_phone_local_songs.md
class PhoneSongsTest {
    private fun op(json: String) = Json.parseToJsonElement(json).jsonObject

    private fun send(
        json: String,
        order: List<Long> = emptyList(),
    ): JsonObject? = Store.forServer(op(json), order)

    // The server rejects an op naming a song it does not know, and the phone
    // drops a rejected op: a phone song left in would lose the whole edit.
    @Test
    fun noPhoneSongReachesTheServer() {
        val ops =
            listOf(
                """{"type":"queue.create","queue":"q","songs":[-5,1,-6,2],"song":-5,"positionMs":0}""",
                """{"type":"queue.replace","queue":"q","songs":[2,-5,1]}""",
                """{"type":"queue.insert","queue":"q","songs":[-7,3],"after":-5}""",
                """{"type":"queue.remove","queue":"q","songs":[-5,1]}""",
                """{"type":"queue.move","queue":"q","song":2,"after":-6}""",
                """{"type":"playlist.create","ref":"r","name":"p","songs":[1,-5]}""",
                """{"type":"playlist.insert","playlist":"7","songs":[-5,4]}""",
            )
        for (o in ops) {
            val text = send(o, order = listOf(1, -5, -6, 2, -7, 3)).toString()
            assertTrue("$o went as $text", !Regex("-\\d").containsMatchIn(text))
        }
        // A queue begun on a phone song starts, on the server, at its first NAS song.
        assertEquals("0", send(ops[0])!!["song"].toString())
    }

    @Test
    fun opsAboutOnlyAPhoneSongAreNotSent() {
        for (o in listOf(
            """{"type":"favorite.set","song":-5}""",
            """{"type":"listen_later.remove","song":-5}""",
            """{"type":"resume.set","song":-5,"positionMs":1000}""",
            """{"type":"play","song":-5,"ms":1000}""",
            """{"type":"queue.set_current","queue":"q","song":-5,"positionMs":0}""",
            """{"type":"queue.move","queue":"q","song":-5,"after":1}""",
            """{"type":"playback.state","queue":"q","song":-5,"positionMs":0,"playing":true}""",
            """{"type":"queue.insert","queue":"q","songs":[-5,-6]}""",
            """{"type":"queue.remove","queue":"q","songs":[-5]}""",
            """{"type":"setting.set","name":"speed.-5","value":{"speed":1.5}}""",
        )) {
            assertNull(o, send(o))
        }
        // NAS ops go as they were.
        val nas = """{"type":"queue.move","queue":"q","song":2,"after":1}"""
        assertEquals(op(nas), send(nas))
        val speed = """{"type":"setting.set","name":"speed.5","value":{"speed":1.5}}"""
        assertEquals(op(speed), send(speed))
    }

    // "after" a phone song becomes the NAS song before it, or the start.
    @Test
    fun afterAPhoneSongIsTheNasSongBeforeIt() {
        val order = listOf<Long>(-1, 1, -5, -6, 2)
        assertEquals("1", send("""{"type":"queue.move","queue":"q","song":2,"after":-6}""", order)!!["after"].toString())
        assertEquals("0", send("""{"type":"queue.move","queue":"q","song":2,"after":-1}""", order)!!["after"].toString())
    }

    // A pull brings the server's order, which never had the phone songs.
    @Test
    fun aPullKeepsPhoneSongsInTheirPlace() {
        val mine = listOf<Long>(-9, 1, -5, -6, 2, 3, -7)
        // The server reordered, and 3 was removed on another device: -7 stays after 2.
        assertEquals(listOf<Long>(-9, 2, -7, 1, -5, -6), mergePhoneSongs(mine, listOf(2, 1)))
        // Every NAS song gone elsewhere: the phone songs stay, from the start.
        assertEquals(listOf<Long>(-9, -5, -6, -7), mergePhoneSongs(mine, emptyList()))
        // No phone songs: the server's list as it is.
        assertEquals(listOf<Long>(3, 1), mergePhoneSongs(listOf(1, 3), listOf(3, 1)))
    }

    @Test
    fun aPhoneIdIsNegativeAndFollowsTheFile() {
        val a = phoneId("external_primary/Music/A/1.mp3")
        assertTrue(a < 0)
        assertEquals(a, phoneId("external_primary/Music/A/1.mp3"))
        assertNotEquals(a, phoneId("external_primary/Music/B/1.mp3"))
        assertTrue((1..2000).all { phoneId("f$it") < 0 })
    }
}
