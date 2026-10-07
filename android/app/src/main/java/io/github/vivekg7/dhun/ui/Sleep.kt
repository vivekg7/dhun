package io.github.vivekg7.dhun.ui

import android.os.SystemClock
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
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
    var custom by remember { mutableStateOf("") }
    val pick = { action: () -> Unit ->
        action()
        onDismiss()
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Sleep timer") },
        text = {
            Column {
                mode?.let { Text("Stops ${sleep.label(it)}.", color = MaterialTheme.colorScheme.primary) }
                SectionLabel("Minutes")
                Chips(listOf(15, 30, 45, 60, 90), 0, { "$it" }) { m -> pick { sleep.minutes(m) } }
                Row(Modifier.padding(start = 20.dp), verticalAlignment = Alignment.CenterVertically) {
                    OutlinedTextField(
                        custom,
                        { custom = it.filter(Char::isDigit).take(3) },
                        Modifier.width(110.dp),
                        placeholder = { Text("Other") },
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                    )
                    Spacer(Modifier.size(8.dp))
                    TextButton({ custom.toIntOrNull()?.let { m -> pick { sleep.minutes(m) } } }, enabled = (custom.toIntOrNull() ?: 0) > 0) { Text("Set") }
                }
                SectionLabel("Songs")
                Chips(listOf(1, 2, 3, 5, 10), 0, { if (it == 1) "End of this song" else "$it songs" }) { n -> pick { sleep.songs(n) } }
                Chips(listOf("queue"), "", { "End of queue" }) { pick { sleep.endOfQueue() } }
            }
        },
        confirmButton = { if (mode != null) TextButton({ pick { sleep.cancel() } }) { Text("Turn off") } },
        dismissButton = { TextButton(onDismiss) { Text("Close") } },
    )
}
