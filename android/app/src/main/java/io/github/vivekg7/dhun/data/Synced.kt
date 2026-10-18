package io.github.vivekg7.dhun.data

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.intOrNull

/**
 * A setting that follows the user to every device: its name in the store,
 * the value it has while unset, and the values Settings offers. A reset
 * unsets it, so a later change of default reaches it
 * (docs/plans/034_settings_text_defaults_reset.md).
 */
class Synced<T> private constructor(
    val name: String,
    val default: T,
    val options: List<T>,
    private val decode: (JsonPrimitive) -> T?,
    private val encode: (T) -> JsonPrimitive,
) {
    /** The user's value in [settings], or [default] when it is missing, null or malformed. */
    fun of(settings: Map<String, JsonElement>): T = (settings[name] as? JsonPrimitive)?.let(decode) ?: default

    fun json(value: T) = encode(value)

    companion object {
        private fun int(
            name: String,
            default: Int,
            vararg options: Int,
        ) = Synced(name, default, options.toList(), { it.intOrNull }, ::JsonPrimitive)

        private fun bool(
            name: String,
            default: Boolean,
        ) = Synced(name, default, listOf(true, false), { it.booleanOrNull }, ::JsonPrimitive)

        private fun string(
            name: String,
            default: String,
            vararg options: String,
        ) = Synced(name, default, options.toList(), { p -> p.content.takeIf { p.isString } }, ::JsonPrimitive)

        // One per line, the name and default together: scripts/check-settings.sh
        // compares them with the Mac's, the web's and the server's.
        val previousRestart = bool("previous.restart", true)
        val longMinMinutes = int("longFiles.minMinutes", 15, 5, 10, 15, 20, 30, 60)
        val longResume = string("longFiles.resume", "auto", "auto", "ask", "off")
        val longPrevious = string("longFiles.previous", "keep", "keep", "seek", "ignore", "songs")
        val longNext = string("longFiles.next", "next", "next", "seek")
        val seekBack = int("longFiles.seekBack", 10, 5, 10, 15, 30, 60)
        val seekForward = int("longFiles.seekForward", 30, 10, 15, 30, 60, 120)
        val rewind = int("longFiles.rewind", 10, 0, 5, 10, 15, 30)
        val rewindAfter = int("longFiles.rewindAfter", 5, 1, 5, 10, 30, 60)
        val laterAutoRemove = bool("listenLater.autoRemove", true)
        val laterFinishedPercent = int("listenLater.finishedPercent", 90, 80, 90, 95, 100)

        // On a show's page, not in Settings, so a reset leaves it.
        val hidePlayed = bool("podcasts.hidePlayed", true)

        /** Every one the Playback page shows, for Reset to defaults. */
        val all =
            listOf(
                previousRestart,
                longMinMinutes,
                longResume,
                longPrevious,
                longNext,
                seekBack,
                seekForward,
                rewind,
                rewindAfter,
                laterAutoRemove,
                laterFinishedPercent,
            )
    }
}
