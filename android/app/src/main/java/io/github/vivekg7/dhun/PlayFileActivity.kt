package io.github.vivekg7.dhun

import android.content.Intent
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Bundle
import android.os.PowerManager
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import io.github.vivekg7.dhun.ui.Icons
import io.github.vivekg7.dhun.ui.Seek
import io.github.vivekg7.dhun.ui.Tip
import io.github.vivekg7.dhun.ui.duration
import io.github.vivekg7.dhun.ui.theme.DhunTheme
import kotlinx.coroutines.delay

/**
 * Plays an audio file another app opens with Dhun, in a dialog over that app
 * (docs/plans/021_open_from_other_apps.md). It has a player of its own: the
 * file is not in the library, so it makes no queue and logs no listen. It
 * takes the audio focus, which pauses the queue where it was; closing the
 * dialog, or leaving it for another app, stops the file.
 */
class PlayFileActivity : ComponentActivity() {
    private lateinit var player: ExoPlayer

    /** Set by a new file; it starts in [onResume], as Android grants the audio focus only to the app in front. */
    private var startPending = false

    /** The file's name, until its tags are read (and for a file without any). */
    private var name by mutableStateOf("")

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        player =
            ExoPlayer
                .Builder(this)
                .setAudioAttributes(
                    AudioAttributes
                        .Builder()
                        .setUsage(C.USAGE_MEDIA)
                        .setContentType(C.AUDIO_CONTENT_TYPE_MUSIC)
                        .build(),
                    true,
                ).setHandleAudioBecomingNoisy(true)
                // Plays on with the screen off.
                .setWakeMode(C.WAKE_MODE_LOCAL)
                .build()
        if (!open(intent)) return
        val prefs = (application as App).prefs
        setContent { DhunTheme(prefs.themeMode, prefs.palette) { FilePlayer(player, name, ::finish) } }
    }

    /** Another file while the dialog shows replaces this one (launchMode singleTop). */
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        open(intent)
    }

    private fun open(intent: Intent): Boolean {
        val uri = intent.data
        if (uri == null) {
            finish()
            return false
        }
        name = displayName(uri)
        player.setMediaItem(MediaItem.fromUri(uri))
        player.prepare()
        startPending = true
        return true
    }

    override fun onResume() {
        super.onResume()
        if (startPending) player.play()
        startPending = false
    }

    private fun displayName(uri: Uri): String =
        runCatching {
            contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
                if (it.moveToFirst()) it.getString(0) else null
            }
        }.getOrNull()
            ?.substringBeforeLast('.')
            ?: uri.lastPathSegment
            ?: "Audio file"

    /**
     * Leaving for another app ends the file: with no notification, nothing
     * could stop it from there. The screen turning off does not.
     */
    override fun onStop() {
        super.onStop()
        if (!isChangingConfigurations && getSystemService(PowerManager::class.java).isInteractive) finish()
    }

    override fun onDestroy() {
        player.release()
        super.onDestroy()
    }
}

@Composable
private fun FilePlayer(
    player: ExoPlayer,
    name: String,
    onClose: () -> Unit,
) {
    val c = MaterialTheme.colorScheme
    var playing by remember { mutableStateOf(player.isPlaying) }
    var meta by remember { mutableStateOf(player.mediaMetadata) }
    // An unreadable file fails before the first frame: read the state, not only its changes.
    var failed by remember { mutableStateOf(player.playerError != null) }
    DisposableEffect(player) {
        val events =
            object : Player.Listener {
                override fun onIsPlayingChanged(isPlaying: Boolean) {
                    playing = isPlaying
                }

                override fun onMediaMetadataChanged(m: MediaMetadata) {
                    meta = m
                }

                override fun onPlayerError(error: PlaybackException) {
                    failed = true
                }

                override fun onMediaItemTransition(
                    item: MediaItem?,
                    reason: Int,
                ) {
                    failed = false
                }
            }
        player.addListener(events)
        onDispose { player.removeListener(events) }
    }
    val art = remember(meta) { meta.artworkData?.let { BitmapFactory.decodeByteArray(it, 0, it.size)?.asImageBitmap() } }

    Surface(Modifier.fillMaxWidth(), RoundedCornerShape(28.dp), c.surfaceContainerHigh, c.onSurface) {
        Column(Modifier.padding(start = 20.dp, end = 8.dp, top = 20.dp, bottom = 16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(
                    Modifier.size(56.dp).clip(RoundedCornerShape(8.dp)).background(c.surfaceContainerHighest),
                    contentAlignment = Alignment.Center,
                ) {
                    if (art != null) {
                        Image(art, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
                    } else {
                        Icon(Icons.Note, null, Modifier.size(24.dp), tint = c.onSurfaceVariant.copy(alpha = 0.5f))
                    }
                }
                Column(Modifier.weight(1f).padding(horizontal = 14.dp)) {
                    Text(
                        meta.title?.toString()?.takeIf { it.isNotBlank() } ?: name,
                        style = MaterialTheme.typography.titleMedium,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                    val line = if (failed) "Dhun can't play this file" else meta.artist?.toString().orEmpty()
                    if (line.isNotBlank()) {
                        Text(
                            line,
                            style = MaterialTheme.typography.bodyMedium,
                            color = if (failed) c.error else c.onSurfaceVariant,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
                Tip("Close") { IconButton(onClose) { Icon(Icons.Close, "Close") } }
            }
            Row(Modifier.padding(top = 12.dp, end = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Tip(if (playing) "Pause" else "Play") {
                    Box(
                        Modifier
                            .size(48.dp)
                            .clip(CircleShape)
                            .background(c.primary)
                            .clickable {
                                if (playing) {
                                    player.pause()
                                } else {
                                    // Heard to the end: play again from the start.
                                    if (player.playbackState == Player.STATE_ENDED) player.seekTo(0)
                                    player.play()
                                }
                            },
                        contentAlignment = Alignment.Center,
                    ) { Icon(if (playing) Icons.Pause else Icons.Play, if (playing) "Pause" else "Play", Modifier.size(28.dp), tint = c.onPrimary) }
                }
                Spacer(Modifier.width(12.dp))
                Progress(player, playing, Modifier.weight(1f))
            }
        }
    }
}

@Composable
private fun Progress(
    player: ExoPlayer,
    playing: Boolean,
    modifier: Modifier,
) {
    val c = MaterialTheme.colorScheme
    var position by remember { mutableLongStateOf(0L) }
    var length by remember { mutableLongStateOf(0L) }
    var dragging by remember { mutableStateOf(false) }
    var dragValue by remember { mutableFloatStateOf(0f) }
    LaunchedEffect(playing) {
        while (true) {
            if (!dragging) position = player.currentPosition
            length = player.duration.coerceAtLeast(0)
            delay(250)
        }
    }
    Column(modifier) {
        Seek(
            fraction =
                if (dragging) {
                    dragValue
                } else if (length > 0) {
                    position.toFloat() / length
                } else {
                    0f
                },
            onDrag = {
                dragging = true
                dragValue = it
            },
            onDone = {
                position = (dragValue * length).toLong()
                player.seekTo(position)
                dragging = false
            },
        )
        Row {
            val shown = if (dragging) (dragValue * length).toLong() else position
            Text(duration(shown), style = MaterialTheme.typography.bodySmall, color = c.onSurfaceVariant)
            Spacer(Modifier.weight(1f))
            Text(duration(length), style = MaterialTheme.typography.bodySmall, color = c.onSurfaceVariant)
        }
    }
}
