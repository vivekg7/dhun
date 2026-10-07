package io.github.vivekg7.dhun.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.play.Tempo
import kotlin.math.abs
import kotlin.math.roundToInt

/**
 * The speed button on Now playing (docs/plans/016_speed_and_pitch.md): an
 * icon at normal speed and pitch, else what is set ("1.5×"), accented.
 */
@Composable
fun SpeedButton() {
    val tempo by App.app.playback.tempo
        .collectAsState()
    var open by remember { mutableStateOf(false) }
    val c = MaterialTheme.colorScheme
    Box(
        Modifier.defaultMinSize(48.dp, 48.dp).clip(CircleShape).clickable { open = true },
        contentAlignment = Alignment.Center,
    ) {
        val label = tempo.label()
        if (label == null) {
            Icon(Icons.Speed, "Speed and pitch", tint = c.onSurfaceVariant)
        } else {
            Text(label, Modifier.padding(horizontal = 6.dp), style = MaterialTheme.typography.labelLarge, color = c.primary)
        }
    }
    if (open) SpeedDialog { open = false }
}

@Composable
private fun SpeedDialog(onDismiss: () -> Unit) {
    val pb = App.app.playback
    val tempo by pb.tempo.collectAsState()
    val own by pb.songTempo.collectAsState()
    val song by pb.current.collectAsState()
    val onlyThis = own != null
    val set = { t: Tempo -> pb.setTempo(t, onlyThis) }
    // The sliders move freely and apply when let go: a per-song change is a synced op.
    var speed by remember(tempo) { mutableFloatStateOf(tempo.speed) }
    var semitones by remember(tempo) { mutableFloatStateOf(tempo.semitones.toFloat()) }
    val c = MaterialTheme.colorScheme
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Speed and pitch") },
        text = {
            Column {
                if (song != null) {
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .heightIn(min = 56.dp)
                            .clickable { if (onlyThis) pb.clearSongTempo() else pb.setTempo(tempo, true) },
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text("Only for this song", style = MaterialTheme.typography.bodyLarge)
                            Text(
                                if (onlyThis) "Kept for this song, on all your devices" else "Off: for every song, on this phone",
                                style = MaterialTheme.typography.bodyMedium,
                                color = c.onSurfaceVariant,
                            )
                        }
                        Switch(onlyThis, null)
                    }
                }
                ValueLabel("Speed", "${Tempo.fmt(speed)}×")
                // No steps: a tick every 0.05 drew thirty dots. The value still snaps to 0.05.
                Slider(
                    speed,
                    { speed = (it * 20).roundToInt() / 20f },
                    valueRange = Tempo.MIN_SPEED..Tempo.MAX_SPEED,
                    onValueChangeFinished = { set(tempo.copy(speed = speed)) },
                )
                Row(Modifier.fillMaxWidth()) {
                    for (p in listOf(0.75f, 1f, 1.25f, 1.5f, 2f)) {
                        val on = p == tempo.speed
                        Box(Modifier.weight(1f).heightIn(min = 40.dp).clickable { set(tempo.copy(speed = p)) }, contentAlignment = Alignment.Center) {
                            Text(
                                "${Tempo.fmt(p)}×",
                                style = MaterialTheme.typography.labelLarge,
                                color = if (on) c.primary else c.onSurfaceVariant,
                                fontWeight = if (on) FontWeight.Bold else null,
                            )
                        }
                    }
                }
                val st = semitones.roundToInt()
                ValueLabel("Pitch", if (st == 0) "Normal" else "${if (st > 0) "+" else "−"}${abs(st)} semitone${if (abs(st) == 1) "" else "s"}")
                Slider(
                    semitones,
                    { semitones = it.roundToInt().toFloat() },
                    valueRange = -Tempo.MAX_SEMITONES.toFloat()..Tempo.MAX_SEMITONES.toFloat(),
                    onValueChangeFinished = { set(tempo.copy(semitones = semitones.roundToInt())) },
                )
            }
        },
        confirmButton = { TextButton(onDismiss) { Text("Done") } },
        dismissButton = { if (!tempo.normal) TextButton({ set(Tempo.NORMAL) }) { Text("Normal") } },
    )
}

/** A slider's name, and its value on the right. */
@Composable
private fun ValueLabel(
    name: String,
    value: String,
) = Row(Modifier.fillMaxWidth().padding(top = 16.dp), verticalAlignment = Alignment.CenterVertically) {
    Text(name, Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge)
    Text(value, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.primary)
}
