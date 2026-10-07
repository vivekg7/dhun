package io.github.vivekg7.dhun.ui

import android.os.SystemClock
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.play.SleepTimer
import kotlinx.coroutines.delay

/** The moon on Now playing: sets the sleep timer, and counts down while one runs (docs/plans/014_sleep_timer.md). */
@Composable
fun SleepButton() {
    val sleep = App.app.playback.sleep
    val mode by sleep.mode.collectAsState()
    var open by remember { mutableStateOf(false) }
    val c = MaterialTheme.colorScheme
    Row(verticalAlignment = Alignment.CenterVertically) {
        IconButton({ open = true }) { Icon(Icons.Sleep, "Sleep timer", tint = if (mode == null) c.onSurfaceVariant else c.primary) }
        when (val m = mode) {
            is SleepTimer.Mode.At -> {
                Countdown(m.endAt)
            }

            is SleepTimer.Mode.Songs -> {
                Text("${m.left}", style = MaterialTheme.typography.labelMedium, color = c.primary)
            }

            else -> {}
        }
    }
    if (open) SleepDialog { open = false }
}

@Composable
private fun Countdown(endAt: Long) {
    var now by remember { mutableLongStateOf(SystemClock.elapsedRealtime()) }
    LaunchedEffect(endAt) {
        while (true) {
            now = SystemClock.elapsedRealtime()
            delay(1000)
        }
    }
    Text(duration((endAt - now).coerceAtLeast(0)), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
}

@Composable
private fun SleepDialog(onDismiss: () -> Unit) {
    val sleep = App.app.playback.sleep
    val mode by sleep.mode.collectAsState()
    // "Other…" asks for a number in a second dialog, in place of this one.
    var other by remember { mutableStateOf<String?>(null) }
    val pick = { action: () -> Unit ->
        action()
        onDismiss()
    }
    when (other) {
        "minutes" -> return NumberDialog("Pause after", "minutes", 20, 1..999, onDismiss) { m -> pick { sleep.minutes(m) } }
        "songs" -> return NumberDialog("Pause after", "songs", 3, 1..999, onDismiss) { n -> pick { sleep.songs(n) } }
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Sleep timer") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                when (val m = mode) {
                    null -> DialogNote("Pauses the music, by the clock or at the end of a song.")
                    else -> DialogNote("Pauses ${sleep.label(m)}.", MaterialTheme.colorScheme.primary)
                }
                DialogLabel("After a time")
                for (m in listOf(15, 30, 45, 60, 90)) DialogRow(timeLabel(m)) { pick { sleep.minutes(m) } }
                DialogRow("Other time…") { other = "minutes" }
                DialogLabel("After songs")
                DialogRow("At the end of this song") { pick { sleep.endOfSong() } }
                DialogRow("After a number of songs…") { other = "songs" }
                DialogRow("At the end of the queue") { pick { sleep.endOfQueue() } }
            }
        },
        confirmButton = { if (mode != null) TextButton({ pick { sleep.cancel() } }) { Text("Turn off") } },
        dismissButton = { TextButton(onDismiss) { Text("Cancel") } },
    )
}

private fun timeLabel(m: Int) =
    when {
        m < 60 -> "$m minutes"
        m == 60 -> "1 hour"
        else -> "1 hour ${m - 60} minutes"
    }
