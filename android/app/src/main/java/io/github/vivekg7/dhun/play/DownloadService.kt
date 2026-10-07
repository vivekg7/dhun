package io.github.vivekg7.dhun.play

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.MainActivity
import io.github.vivekg7.dhun.R
import io.github.vivekg7.dhun.data.Downloads
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.sample
import kotlinx.coroutines.launch

/**
 * Keeps the process alive while downloads run, and shows their progress
 * (docs/plans/012_downloads.md). The work itself is in [Downloads]; this
 * only mirrors its status and stops when there is nothing to fetch.
 */
class DownloadService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private val downloads get() = (application as App).downloads

    override fun onCreate() {
        super.onCreate()
        running = true
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(CHANNEL, "Downloads", NotificationManager.IMPORTANCE_LOW),
        )
    }

    override fun onStartCommand(
        intent: Intent?,
        flags: Int,
        startId: Int,
    ): Int {
        startForeground(ID, notification(downloads.status.value), ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        scope.launch {
            // At most twice a second: a notification per 512 KB is more than the system allows.
            downloads.status.sample(500).collect { s ->
                if (s.state != Downloads.State.Downloading) {
                    stopForeground(STOP_FOREGROUND_REMOVE)
                    stopSelf()
                } else {
                    getSystemService(NotificationManager::class.java).notify(ID, notification(s))
                }
            }
        }
        return START_NOT_STICKY
    }

    /** Android 15 allows dataSync six hours a day; what is left waits for the next start. */
    override fun onTimeout(
        startId: Int,
        fgsType: Int,
    ) = stopSelf()

    override fun onDestroy() {
        running = false
        scope.cancel()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?) = null

    private fun notification(s: Downloads.Status): Notification {
        val open =
            PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        return Notification
            .Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("Downloading ${(s.done + 1).coerceAtMost(s.wanted)} of ${s.wanted}")
            .setContentText(s.current?.title ?: "")
            .setProgress(100, (s.progress * 100).toInt(), s.current == null)
            .setContentIntent(open)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .build()
    }

    companion object {
        private const val CHANNEL = "downloads"
        private const val ID = 2

        /** Set while the service is up, so [Downloads] does not start it again for every song. */
        @Volatile var running = false
    }
}
