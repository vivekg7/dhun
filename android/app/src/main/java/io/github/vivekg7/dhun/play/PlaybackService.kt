package io.github.vivekg7.dhun.play

import android.app.PendingIntent
import android.content.Intent
import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSourceBitmapLoader
import androidx.media3.datasource.okhttp.OkHttpDataSource
import androidx.media3.session.DefaultMediaNotificationProvider
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.MainActivity
import io.github.vivekg7.dhun.R

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
                .Builder(this, app.playback.player)
                .setSessionActivity(open)
                // Cover art needs the device token, so it goes through our client.
                .setBitmapLoader(DataSourceBitmapLoader.Builder(this).setDataSourceFactory(OkHttpDataSource.Factory(app.api.http)).build())
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
        session?.release()
        session = null
        super.onDestroy()
    }
}
