package io.github.vivekg7.dhun.play

import android.app.PendingIntent
import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import androidx.annotation.OptIn
import androidx.media3.common.FlagSet
import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.common.util.BitmapLoader
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSourceBitmapLoader
import androidx.media3.datasource.okhttp.OkHttpDataSource
import androidx.media3.session.DefaultMediaNotificationProvider
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.SettableFuture
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.MainActivity
import io.github.vivekg7.dhun.R
import io.github.vivekg7.dhun.data.Covers
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
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

    override fun onCreate() {
        super.onCreate()
        val app = application as App
        val open =
            PendingIntent.getActivity(
                this,
                0,
                Intent(this, MainActivity::class.java),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
        session =
            MediaSession
                .Builder(this, SleepAware(app.playback.player, app.playback.sleep))
                .setSessionActivity(open)
                .setBitmapLoader(CoverLoader(DataSourceBitmapLoader.Builder(this).setDataSourceFactory(OkHttpDataSource.Factory(app.api.http)).build()))
                .build()
        setMediaNotificationProvider(DefaultMediaNotificationProvider.Builder(this).build().apply { setSmallIcon(R.drawable.ic_stat) })
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo) = session

    /** Swiping the app away stops the service only when nothing is playing. */
    override fun onTaskRemoved(rootIntent: Intent?) {
        val player = session?.player ?: return stopSelf()
        if (!player.playWhenReady || player.mediaItemCount == 0) stopSelf()
    }

    override fun onDestroy() {
        (session?.player as? SleepAware)?.close()
        session?.release()
        session = null
        super.onDestroy()
    }
}

/**
 * The player as the session sees it: with a sleep timer set, the artist line
 * says so ("Coldplay · Sleep in 23 min"), so the notification and the lock
 * screen show it (docs/plans/014_sleep_timer.md). Refreshed every half
 * minute while a timer runs.
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
