package io.github.vivekg7.dhun.play

import android.os.SystemClock
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

/**
 * Stops playback later (docs/plans/014_sleep_timer.md): after some minutes,
 * at the end of this song, after a number of songs, at the end of the
 * queue, or after a song picked in the queue (docs/plans/020_queue_screen.md). A timer by the clock fades out over its last seconds and pauses on
 * time; the others pause where a song ends, which needs no fade.
 */
@OptIn(UnstableApi::class)
class SleepTimer(
    private val player: ExoPlayer,
    private val scope: CoroutineScope,
) {
    sealed interface Mode {
        /** At [endAt], in [SystemClock.elapsedRealtime] (unaffected by clock changes). */
        data class At(
            val endAt: Long,
        ) : Mode

        data object EndOfSong : Mode

        /** [left] songs to go, the playing one included. */
        data class Songs(
            val left: Int,
        ) : Mode

        data object EndOfQueue : Mode

        /** Musicolet's "Stop after this song", for a song further down the queue. */
        data class AfterSong(
            val song: Long,
            val title: String,
        ) : Mode
    }

    private val _mode = MutableStateFlow<Mode?>(null)
    val mode: StateFlow<Mode?> = _mode
    private var fading: Job? = null

    fun minutes(m: Int) {
        val endAt = SystemClock.elapsedRealtime() + m * 60_000L
        set(Mode.At(endAt))
        fading =
            scope.launch {
                delay((endAt - FADE_MS - SystemClock.elapsedRealtime()).coerceAtLeast(0))
                while (true) {
                    val left = endAt - SystemClock.elapsedRealtime()
                    if (left <= 0) break
                    player.volume = (left.toFloat() / FADE_MS).coerceIn(0f, 1f)
                    delay(100)
                }
                player.pause()
                cancel()
            }
    }

    fun endOfSong() = set(Mode.EndOfSong)

    fun songs(n: Int) = set(if (n <= 1) Mode.EndOfSong else Mode.Songs(n))

    fun endOfQueue() = set(Mode.EndOfQueue)

    fun afterSong(
        song: Long,
        title: String,
    ) = set(Mode.AfterSong(song, title))

    fun cancel() {
        fading?.cancel()
        fading = null
        player.volume = 1f
        player.pauseAtEndOfMediaItems = false
        _mode.value = null
    }

    private fun set(m: Mode) {
        cancel()
        _mode.value = m
        update()
    }

    /** A song started (auto or skipped to): one fewer to go, and maybe this is the last. */
    fun onNextSong() {
        val m = _mode.value
        if (m is Mode.Songs) _mode.value = if (m.left <= 2) Mode.EndOfSong else m.copy(left = m.left - 1)
        update()
    }

    /** The player paused itself at the end of the last song. */
    fun onPausedAtEnd() {
        if (_mode.value != null && _mode.value !is Mode.At) cancel()
    }

    /** Asks the player to pause where the playing song ends, when it is the last one. */
    fun update() {
        player.pauseAtEndOfMediaItems =
            when (val m = _mode.value) {
                is Mode.AfterSong -> {
                    player.currentMediaItem?.mediaId == m.song.toString()
                }

                Mode.EndOfSong -> {
                    true
                }

                Mode.EndOfQueue -> {
                    player.currentTimeline.getNextWindowIndex(player.currentMediaItemIndex, Player.REPEAT_MODE_OFF, player.shuffleModeEnabled) ==
                        C.INDEX_UNSET
                }

                else -> {
                    false
                }
            }
    }

    /** "in 23 min", "after this song", …, or null with no timer: for the notification and the button. */
    fun label(m: Mode? = _mode.value): String? =
        when (m) {
            null -> null
            is Mode.At -> "in ${((m.endAt - SystemClock.elapsedRealtime() + 59_999) / 60_000).coerceAtLeast(1)} min"
            Mode.EndOfSong -> "after this song"
            is Mode.Songs -> "after ${m.left} songs"
            Mode.EndOfQueue -> "at the end of the queue"
            is Mode.AfterSong -> "after “${m.title}”"
        }

    companion object {
        /** How long the volume takes to fall to nothing before a timer by the clock pauses. */
        const val FADE_MS = 10_000L
    }
}
