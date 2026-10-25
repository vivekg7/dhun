package io.github.vivekg7.dhun.play

import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.view.KeyEvent
import android.view.ViewConfiguration
import androidx.annotation.OptIn
import androidx.core.content.IntentCompat
import androidx.media3.common.FlagSet
import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.common.util.BitmapLoader
import androidx.media3.common.util.UnstableApi
import androidx.media3.common.util.Util
import androidx.media3.datasource.DataSourceBitmapLoader
import androidx.media3.datasource.okhttp.OkHttpDataSource
import androidx.media3.session.CommandButton
import androidx.media3.session.DefaultMediaNotificationProvider
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.SettableFuture
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.R
import io.github.vivekg7.dhun.data.Covers
import io.github.vivekg7.dhun.data.Synced
import io.github.vivekg7.dhun.openApp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
import java.io.IOException

/**
 * The media session around [Playback]'s player: the notification, the lock
 * screen, Bluetooth and headset buttons. Media3 keeps the service in the
 * foreground while something plays.
 *
 * Opts in to Media3's "unstable" APIs (they may change between versions),
 * which cover much of ExoPlayer and the session.
 */
@OptIn(UnstableApi::class)
class PlaybackService : MediaSessionService() {
    private var session: MediaSession? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)

    override fun onCreate() {
        super.onCreate()
        val app = application as App
        session =
            MediaSession
                .Builder(this, SleepAware(app.playback.player, app.playback.sleep))
                .setSessionActivity(openApp())
                .setCallback(SessionCallback())
                .setBitmapLoader(CoverLoader(DataSourceBitmapLoader.Builder(this).setDataSourceFactory(OkHttpDataSource.Factory(app.api.http)).build()))
                .build()
        setMediaNotificationProvider(DefaultMediaNotificationProvider.Builder(this).build().apply { setSmallIcon(R.drawable.ic_stat) })
        scope.launch {
            combine(app.playback.current, app.store.settings) { _, _ -> buttons(app) }
                .distinctUntilChanged()
                .collect { session?.setMediaButtonPreferences(it) }
        }
    }

    /**
     * A long file's notification and lock screen seek either side of Play,
     * as Now playing does, and keep Previous and Next in the overflow;
     * a song's keep Media3's own, Previous and Next.
     */
    private fun buttons(app: App): List<CommandButton> {
        val s = app.playback.current.value
        if (s == null || !app.playback.isLong(s)) return emptyList()
        val settings = app.store.settings.value
        val back = Synced.seekBack.of(settings)
        val on = Synced.seekForward.of(settings)

        fun button(
            icon: Int,
            name: String,
            command: Int,
            slot: Int,
        ) = CommandButton
            .Builder(icon)
            .setDisplayName(name)
            .setPlayerCommand(command)
            .setSlots(slot)
            .build()
        return listOf(
            button(BACK_ICONS[back] ?: CommandButton.ICON_SKIP_BACK, "Back $back seconds", Player.COMMAND_SEEK_BACK, CommandButton.SLOT_BACK),
            button(FORWARD_ICONS[on] ?: CommandButton.ICON_SKIP_FORWARD, "Forward $on seconds", Player.COMMAND_SEEK_FORWARD, CommandButton.SLOT_FORWARD),
            button(CommandButton.ICON_PREVIOUS, "Previous", Player.COMMAND_SEEK_TO_PREVIOUS, CommandButton.SLOT_OVERFLOW),
            button(CommandButton.ICON_NEXT, "Next", Player.COMMAND_SEEK_TO_NEXT, CommandButton.SLOT_OVERFLOW),
        )
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo) = session

    /** Swiping the app away stops the service only when nothing is playing. */
    override fun onTaskRemoved(rootIntent: Intent?) {
        val player = session?.player ?: return stopSelf()
        if (!player.playWhenReady || player.mediaItemCount == 0) stopSelf()
    }

    override fun onDestroy() {
        scope.cancel()
        (session?.player as? SleepAware)?.close()
        session?.release()
        session = null
        super.onDestroy()
    }
}

/** The seek icons with their seconds drawn in; any other number gets the plain arrow. */
private val BACK_ICONS =
    mapOf(
        5 to CommandButton.ICON_SKIP_BACK_5,
        10 to CommandButton.ICON_SKIP_BACK_10,
        15 to CommandButton.ICON_SKIP_BACK_15,
        30 to CommandButton.ICON_SKIP_BACK_30,
    )
private val FORWARD_ICONS =
    mapOf(
        5 to CommandButton.ICON_SKIP_FORWARD_5,
        10 to CommandButton.ICON_SKIP_FORWARD_10,
        15 to CommandButton.ICON_SKIP_FORWARD_15,
        30 to CommandButton.ICON_SKIP_FORWARD_30,
    )

/**
 * The player as the session sees it: with a sleep timer set, the artist line
 * says so ("Coldplay · Sleep in 23 min"), so the notification and the lock
 * screen show it (docs/plans/014_sleep_timer.md). Refreshed every half
 * minute while a timer runs. Play, Previous, Next and seeking go to [Playback],
 * which keeps a long file's place from a headset
 * (docs/plans/009_resume_long_files.md).
 */
@OptIn(UnstableApi::class)
private class SleepAware(
    player: Player,
    private val sleep: SleepTimer,
) : ForwardingPlayer(player) {
    private val listeners = mutableListOf<Player.Listener>()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)

    init {
        scope.launch {
            sleep.mode.collectLatest { mode ->
                do {
                    refresh()
                    delay(30_000)
                } while (mode != null)
            }
        }
    }

    override fun getMediaMetadata(): MediaMetadata {
        val m = super.getMediaMetadata()
        val label = sleep.label() ?: return m
        val artist = m.artist?.let { "$it · " } ?: ""
        return m.buildUpon().setArtist("${artist}Sleep $label").build()
    }

    // Goes back first after a long pause, before any sound (plan 009).
    override fun play() = App.app.playback.play()

    override fun setPlayWhenReady(playWhenReady: Boolean) = if (playWhenReady) play() else super.setPlayWhenReady(false)

    override fun seekToPrevious() = App.app.playback.previous(onScreen = false)

    override fun seekToNext() = App.app.playback.next(onScreen = false)

    // The notification's and lock screen's seek buttons, by the seconds set for long files.
    override fun seekBack() = App.app.playback.seekBack()

    override fun seekForward() = App.app.playback.seekForward()

    override fun addListener(listener: Player.Listener) {
        listeners += listener
        super.addListener(listener)
    }

    override fun removeListener(listener: Player.Listener) {
        listeners -= listener
        super.removeListener(listener)
    }

    private fun refresh() {
        val events = Player.Events(FlagSet.Builder().add(Player.EVENT_MEDIA_METADATA_CHANGED).build())
        for (l in listeners.toList()) {
            l.onMediaMetadataChanged(mediaMetadata)
            l.onEvents(this, events)
        }
    }

    /** Stops the refresh; the player itself lives on with the process ([Playback]). */
    fun close() = scope.cancel()
}

/**
 * What other controllers ask of the session, beyond the player's own commands.
 *
 * A wired headset's button sends each press as it comes, and the app counts
 * them: one plays or pauses, two go to the next song, three to the previous,
 * once the presses stop. Media3 counts only to two, so a third press paused.
 * Buds count their own taps and send Next or Previous, which never get here.
 * The notification's Play sends the same key, and acts at once.
 *
 * "Play X on Dhun" while the app runs arrives as the assistant's
 * playFromSearch, which Media3 hands on as items to set with a search query
 * in their request metadata (docs/plans/038_voice_search_and_assistant.md).
 */
@OptIn(UnstableApi::class)
private class SessionCallback : MediaSession.Callback {
    private val handler = Handler(Looper.getMainLooper())
    private var presses = 0

    override fun onSetMediaItems(
        mediaSession: MediaSession,
        controller: MediaSession.ControllerInfo,
        mediaItems: List<MediaItem>,
        startIndex: Int,
        startPositionMs: Long,
    ): ListenableFuture<MediaSession.MediaItemsWithStartPosition> {
        val asked =
            mediaItems.firstOrNull { it.requestMetadata.searchQuery != null }
                ?: return super.onSetMediaItems(mediaSession, controller, mediaItems, startIndex, startPositionMs)
        App.app.intents.play(asked.requestMetadata.searchQuery!!, asked.requestMetadata.extras)
        // The queue is Playback's to make. A failed future tells Media3 the session
        // chose to ignore the request, and it leaves the player alone.
        return Futures.immediateFailedFuture(UnsupportedOperationException("Dhun plays it as a queue"))
    }

    /**
     * A headset button, or Bluetooth, with no player running: the receiver
     * in the manifest starts the service, and Media3 asks for something to
     * play. Playback restores the last queue itself, so this waits for it
     * and presses Play; the future fails on purpose, since the items it
     * would carry are already in the player, and Media3 then only calls
     * play on a player that is playing.
     */
    override fun onPlaybackResumption(
        mediaSession: MediaSession,
        controller: MediaSession.ControllerInfo,
        isForPlayback: Boolean,
    ): ListenableFuture<MediaSession.MediaItemsWithStartPosition> {
        val done = SettableFuture.create<MediaSession.MediaItemsWithStartPosition>()
        if (!isForPlayback) {
            done.setException(UnsupportedOperationException("No resumption notification without a library service"))
            return done
        }
        App.app.scope.launch {
            try {
                App.app.intents.resume(show = false)
            } finally {
                done.setException(IllegalStateException("Dhun restored its own queue"))
            }
        }
        return done
    }

    override fun onMediaButtonEvent(
        session: MediaSession,
        controller: MediaSession.ControllerInfo,
        intent: Intent,
    ): Boolean {
        val key = IntentCompat.getParcelableExtra(intent, Intent.EXTRA_KEY_EVENT, KeyEvent::class.java) ?: return false
        if (key.keyCode != KeyEvent.KEYCODE_HEADSETHOOK && key.keyCode != KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE) return false
        if (session.isMediaNotificationController(controller)) return false
        if (key.action == KeyEvent.ACTION_DOWN && key.repeatCount == 0) {
            presses++
            handler.removeCallbacksAndMessages(null)
            handler.postDelayed({ act(session.player) }, ViewConfiguration.getDoubleTapTimeout().toLong())
        }
        return true
    }

    private fun act(player: Player) {
        when (presses) {
            1 -> Util.handlePlayPauseButtonAction(player)
            2 -> player.seekToNext()
            else -> player.seekToPrevious()
        }
        presses = 0
    }
}

/**
 * The notification's and lock screen's cover comes from [Covers], so it is
 * fetched once with the app's and shows offline. Anything else (none yet)
 * goes to [other], through our client for the device token.
 */
@OptIn(UnstableApi::class)
private class CoverLoader(
    private val other: BitmapLoader,
) : BitmapLoader by other {
    override fun loadBitmap(uri: Uri): ListenableFuture<Bitmap> {
        val app = App.app
        val song =
            uri.lastPathSegment
                ?.toLongOrNull()
                ?.takeIf { uri.toString() == app.api.artUrl(it, Playback.COVER_PX) }
                ?.let { app.catalog.value.byId[it] }
                ?: return other.loadBitmap(uri)
        val result = SettableFuture.create<Bitmap>()
        app.scope.launch {
            val bitmap = Covers.load(song, Playback.COVER_PX)
            if (bitmap != null) result.set(bitmap) else result.setException(IOException("No cover for ${song.id}"))
        }
        return result
    }
}
