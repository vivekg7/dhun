package io.github.vivekg7.dhun

import io.github.vivekg7.dhun.play.Tempo
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.util.Locale

class TempoTest {
    // A phone in German would otherwise show "1,25×".
    @Test
    fun speedsReadTheSameInEveryLocale() {
        val was = Locale.getDefault()
        Locale.setDefault(Locale.GERMANY)
        try {
            assertEquals(listOf("0.75", "1", "1.25", "1.5", "2"), listOf(0.75f, 1f, 1.25f, 1.5f, 2f).map(Tempo::fmt))
        } finally {
            Locale.setDefault(was)
        }
    }

    // A song's setting comes from another device, maybe a newer app: never trusted to be in range.
    @Test
    fun aSongSettingIsReadDefensively() {
        assertEquals(Tempo(1.5f, -2), Tempo.of(Json.parseToJsonElement("""{"speed":1.5,"semitones":-2}""")))
        assertEquals(Tempo(2f, 6), Tempo.of(Json.parseToJsonElement("""{"speed":9,"semitones":40}""")))
        assertEquals(Tempo(1.25f, 0), Tempo.of(Json.parseToJsonElement("""{"speed":1.25}""")))
        assertNull(Tempo.of(JsonPrimitive(1.5)))
        assertNull(Tempo.of(null))
    }
}
