package io.github.vivekg7.dhun.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
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
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.play.Tempo
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
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Speed and pitch") },
        text = {
            Column {
                if (song != null) {
                    Row(
                        Modifier.clickable { if (onlyThis) pb.clearSongTempo() else pb.setTempo(tempo, true) },
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Checkbox(onlyThis, null, Modifier.padding(12.dp))
                        Column {
                            Text("Only for this song")
                            Text(
                                if (onlyThis) "Kept for “${song?.title}” on all your devices" else "Otherwise for every song, on this phone",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
                SectionLabel("Speed · ${Tempo.fmt(speed)}×")
                Slider(
                    speed,
                    { speed = (it * 20).roundToInt() / 20f },
                    Modifier.padding(horizontal = 12.dp),
                    valueRange = Tempo.MIN_SPEED..Tempo.MAX_SPEED,
                    // 0.05 apart.
                    steps = ((Tempo.MAX_SPEED - Tempo.MIN_SPEED) * 20).roundToInt() - 1,
                    onValueChangeFinished = { set(tempo.copy(speed = speed)) },
                )
                Chips(listOf(0.75f, 1f, 1.25f, 1.5f, 2f), tempo.speed, { "${Tempo.fmt(it)}×" }) { set(tempo.copy(speed = it)) }
                val st = semitones.roundToInt()
                SectionLabel(
                    "Pitch · " +
                        if (st == 0) "normal" else "${if (st > 0) "+" else "−"}${kotlin.math.abs(st)} semitone${if (kotlin.math.abs(st) == 1) "" else "s"}",
                )
                Slider(
                    semitones,
                    { semitones = it.roundToInt().toFloat() },
                    Modifier.padding(horizontal = 12.dp),
                    valueRange = -Tempo.MAX_SEMITONES.toFloat()..Tempo.MAX_SEMITONES.toFloat(),
                    steps = Tempo.MAX_SEMITONES * 2 - 1,
                    onValueChangeFinished = { set(tempo.copy(semitones = semitones.roundToInt())) },
                )
            }
        },
        confirmButton = { TextButton(onDismiss) { Text("Close") } },
        dismissButton = { if (!tempo.normal) TextButton({ set(Tempo.NORMAL) }) { Text("Normal") } },
    )
}
