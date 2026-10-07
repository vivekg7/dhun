package io.github.vivekg7.dhun.ui

import android.os.SystemClock
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
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
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.play.SleepTimer
import kotlinx.coroutines.delay

/**
 * The moon on Now playing, only while a timer runs: it counts down, and a tap
 * opens the timer. The timer is set from the menu (docs/plans/014_sleep_timer.md).
 */
@Composable
fun SleepButton() {
    val mode by App.app.playback.sleep.mode
        .collectAsState()
    var open by remember { mutableStateOf(false) }
    if (open) SleepDialog { open = false }
    if (mode == null) return
    val c = MaterialTheme.colorScheme
    Row(Modifier.clickable { open = true }, verticalAlignment = Alignment.CenterVertically) {
        Tip("Sleep timer") { IconButton({ open = true }) { Icon(Icons.Sleep, "Sleep timer", tint = c.primary) } }
        Text(sleepState() ?: "", Modifier.padding(end = 8.dp), style = MaterialTheme.typography.labelMedium, color = c.primary)
    }
}

/** What a running timer has left, kept current: "23:10", or "3 songs", or null with none set. */
@Composable
fun sleepState(): String? {
    val mode by App.app.playback.sleep.mode
        .collectAsState()
    var now by remember { mutableLongStateOf(SystemClock.elapsedRealtime()) }
    val m = mode
    LaunchedEffect(m) {
        while (m is SleepTimer.Mode.At) {
            now = SystemClock.elapsedRealtime()
            delay(1000)
        }
    }
    return when (m) {
        null -> null
        is SleepTimer.Mode.At -> duration((m.endAt - now).coerceAtLeast(0))
        SleepTimer.Mode.EndOfSong -> "this song"
        is SleepTimer.Mode.Songs -> "${m.left} songs"
        SleepTimer.Mode.EndOfQueue -> "end of queue"
    }
}

@Composable
fun SleepDialog(onDismiss: () -> Unit) {
    val sleep = App.app.playback.sleep
    val mode by sleep.mode.collectAsState()
    var pick by remember { mutableStateOf(Pick.Time) }
    var hours by remember { mutableStateOf("00") }
    var mins by remember { mutableStateOf("30") }
    var songs by remember { mutableStateOf("5") }
    val total = (hours.toIntOrNull() ?: 0) * 60 + (mins.toIntOrNull() ?: 0)
    val count = songs.toIntOrNull() ?: 0

    // The arrows move the whole time, so 00:55 up is 01:00 and 01:00 down is 00:55.
    fun setTotal(m: Int) {
        val t = m.coerceIn(0, MAX_MINUTES)
        hours = two(t / 60)
        mins = two(t % 60)
    }

    val ok =
        when (pick) {
            Pick.Time -> total in 1..MAX_MINUTES
            Pick.Songs -> count in 1..MAX_SONGS
            Pick.Queue -> true
        }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Sleep timer") },
        text = {
            Column {
                when (val m = mode) {
                    null -> DialogNote("Pauses the music when the time is up or the songs are done.")
                    else -> DialogNote("Pauses ${sleep.label(m)}.", MaterialTheme.colorScheme.primary)
                }
                Choice("After a time", "hours : minutes", pick == Pick.Time, { pick = Pick.Time }) {
                    Stepper(hours, { hours = it }, { setTotal(total + 60) }, { setTotal(total - 60) }) { pick = Pick.Time }
                    Text(":", Modifier.padding(horizontal = 4.dp), style = MaterialTheme.typography.titleLarge)
                    Stepper(mins, { mins = it }, { setTotal(total / 5 * 5 + 5) }, { setTotal((total + 4) / 5 * 5 - 5) }) { pick = Pick.Time }
                }
                Choice("After songs", "1 is the end of this song", pick == Pick.Songs, { pick = Pick.Songs }) {
                    Stepper(songs, { songs = it }, { songs = "${(count + 1).coerceAtMost(MAX_SONGS)}" }, { songs = "${(count - 1).coerceAtLeast(1)}" }) {
                        pick = Pick.Songs
                    }
                }
                Choice("At the end of this queue", null, pick == Pick.Queue, { pick = Pick.Queue })
            }
        },
        confirmButton = {
            TextButton({
                when (pick) {
                    Pick.Time -> sleep.minutes(total)
                    Pick.Songs -> sleep.songs(count)
                    Pick.Queue -> sleep.endOfQueue()
                }
                onDismiss()
            }, enabled = ok) { Text("Start") }
        },
        dismissButton = {
            Row {
                if (mode != null) {
                    TextButton({
                        sleep.cancel()
                        onDismiss()
                    }) { Text("Turn off") }
                }
                TextButton(onDismiss) { Text("Cancel") }
            }
        },
    )
}

private enum class Pick { Time, Songs, Queue }

private const val MAX_MINUTES = 23 * 60 + 55
private const val MAX_SONGS = 99

private fun two(n: Int) = n.toString().padStart(2, '0')

/** One way to stop: its radio button and name on the left, its value on the right. */
@Composable
private fun Choice(
    title: String,
    hint: String?,
    selected: Boolean,
    onSelect: () -> Unit,
    value: @Composable () -> Unit = {},
) = Row(
    Modifier
        .fillMaxWidth()
        .heightIn(min = 56.dp)
        .clickable(onClick = onSelect)
        .padding(vertical = 4.dp),
    verticalAlignment = Alignment.CenterVertically,
) {
    RadioButton(selected, null)
    Spacer(Modifier.width(12.dp))
    Column(Modifier.weight(1f)) {
        Text(title, style = MaterialTheme.typography.bodyLarge)
        hint?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
    value()
}

/** A number box with a small step button above and below it, as number inputs have. Typing picks its choice too. */
@Composable
private fun Stepper(
    text: String,
    onText: (String) -> Unit,
    up: () -> Unit,
    down: () -> Unit,
    onTouch: () -> Unit,
) = Column(horizontalAlignment = Alignment.CenterHorizontally) {
    val c = MaterialTheme.colorScheme
    Tip("More") {
        IconButton({
            onTouch()
            up()
        }, Modifier.size(32.dp)) { Icon(Icons.ExpandLess, "More", tint = c.onSurfaceVariant) }
    }
    OutlinedTextField(
        text,
        {
            onTouch()
            onText(it.filter(Char::isDigit).take(2))
        },
        Modifier.width(64.dp),
        textStyle = MaterialTheme.typography.titleMedium.copy(textAlign = TextAlign.Center),
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
    )
    Tip("Less") {
        IconButton({
            onTouch()
            down()
        }, Modifier.size(32.dp)) { Icon(Icons.ExpandMore, "Less", tint = c.onSurfaceVariant) }
    }
}
