package io.github.vivekg7.dhun

import android.content.ComponentName
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import com.google.common.util.concurrent.ListenableFuture
import io.github.vivekg7.dhun.play.PlaybackService
import io.github.vivekg7.dhun.ui.Shell
import io.github.vivekg7.dhun.ui.SignInScreen
import io.github.vivekg7.dhun.ui.theme.DhunTheme

class MainActivity : ComponentActivity() {
    private var controller: ListenableFuture<MediaController>? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val app = application as App
        setContent {
            DhunTheme(app.prefs.themeMode, app.prefs.palette) {
                // The Surface sets the text and icon colour for everything inside.
                Surface(color = MaterialTheme.colorScheme.background) {
                    if (app.prefs.token.isEmpty() && !app.prefs.withoutAccount) SignInScreen() else Shell()
                }
            }
        }
    }

    /**
     * Connecting a controller starts the playback service, which owns the
     * notification. The UI itself talks to the player directly: it is in
     * this process (App.playback).
     */
    override fun onStart() {
        super.onStart()
        controller = MediaController.Builder(this, SessionToken(this, ComponentName(this, PlaybackService::class.java))).buildAsync()
    }

    override fun onStop() {
        controller?.let(MediaController::releaseFuture)
        controller = null
        super.onStop()
    }
}
