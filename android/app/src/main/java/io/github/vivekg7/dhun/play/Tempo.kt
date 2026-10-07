package io.github.vivekg7.dhun.play

import androidx.media3.common.PlaybackParameters
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.floatOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.put
import java.util.Locale
import kotlin.math.pow
import kotlin.math.roundToInt

/**
 * Play speed and pitch (docs/plans/016_speed_and_pitch.md), independent of
 * each other: a faster speed keeps the voice's pitch, and the pitch moves
 * in semitones, the steps a singer thinks in.
 */
data class Tempo(
    val speed: Float = 1f,
    val semitones: Int = 0,
) {
    val normal get() = this == NORMAL

    val params get() = PlaybackParameters(speed, 2f.pow(semitones / 12f))

    fun json(): JsonElement =
        buildJsonObject {
            put("speed", speed)
            put("semitones", semitones)
        }

    /**
     * What the button on Now playing shows, or null when normal: the speed
     * ("1.5×"), else the pitch ("+2"). One value keeps the row of buttons
     * on a phone's width next to a running sleep timer; the dialog has both.
     */
    fun label(): String? =
        when {
            speed != 1f -> "${fmt(speed)}×"
            semitones > 0 -> "+$semitones"
            semitones < 0 -> "−${-semitones}"
            else -> null
        }

    companion object {
        val NORMAL = Tempo()
        const val MIN_SPEED = 0.5f
        const val MAX_SPEED = 2f
        const val MAX_SEMITONES = 6

        /** A song's setting (`speed.<id>`), or null when it has none or it cannot be read. */
        fun of(v: JsonElement?): Tempo? {
            val o = v as? JsonObject ?: return null
            val speed = (o["speed"] as? JsonPrimitive)?.floatOrNull ?: 1f
            val semitones = (o["semitones"] as? JsonPrimitive)?.intOrNull ?: 0
            return Tempo(speed.coerceIn(MIN_SPEED, MAX_SPEED), semitones.coerceIn(-MAX_SEMITONES, MAX_SEMITONES))
        }

        fun songSetting(song: Long) = "speed.$song"

        /** 1.25 → "1.25", 1.5 → "1.5", 2 → "2". */
        fun fmt(speed: Float): String {
            val hundredths = (speed * 100).roundToInt()
            return if (hundredths % 100 == 0) "${hundredths / 100}" else String.format(Locale.ROOT, "%.2f", hundredths / 100f).trimEnd('0')
        }
    }
}
