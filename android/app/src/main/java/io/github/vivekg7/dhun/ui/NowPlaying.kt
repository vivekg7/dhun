package io.github.vivekg7.dhun.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Store
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@Composable
fun NowPlayingScreen(nav: Nav) {
    val app = App.app
    val pb = app.playback
    val song by pb.current.collectAsState()
    val playing by pb.playing.collectAsState()
    val queue by pb.active.collectAsState()
    val queues by pb.queues.collectAsState()
    val offer by pb.offerResume.collectAsState()
    val favs by app.store.favorites.collectAsState(emptyList())
    val later by app.store.listenLater.collectAsState(emptyList())
    val scope = rememberCoroutineScope()
    val c = MaterialTheme.colorScheme
    var lyrics by rememberSaveable { mutableStateOf(false) }
    // Lyrics are read at arm's length: the screen stays on while they show and the song plays.
    val view = LocalView.current
    DisposableEffect(view, lyrics && playing) {
        view.keepScreenOn = lyrics && playing
        onDispose { view.keepScreenOn = false }
    }
    val s = song
    if (s == null) {
        Empty("Nothing is playing.\nPick something from your library.")
        return
    }

    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier.fillMaxWidth().clickable { nav.tab = Tab.Queues }.padding(horizontal = 20.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val n = queues.indexOfFirst { it.id == queue?.id } + 1
            Text("QUEUE $n OF ${queues.size} · ", style = MaterialTheme.typography.labelSmall, color = c.onSurfaceVariant)
            Text(
                queue?.name ?: "",
                style = MaterialTheme.typography.labelLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            Icon(Icons.ExpandMore, null, Modifier.size(18.dp), tint = c.onSurfaceVariant)
        }

        // The cover takes what height is left, up to a square; a tap swaps it for the lyrics.
        BoxWithConstraints(Modifier.weight(1f).fillMaxWidth().padding(horizontal = 24.dp), contentAlignment = Alignment.Center) {
            if (lyrics) {
                LyricsView(s) { lyrics = false }
            } else {
                Art(
                    s,
                    minOf(maxWidth, maxHeight),
                    RoundedCornerShape(16.dp),
                    Modifier.clickable(interactionSource = null, indication = null) { lyrics = true },
                )
            }
        }

        Column(Modifier.padding(start = 24.dp, end = 24.dp, top = 20.dp)) {
            Text(s.title, style = MaterialTheme.typography.titleLarge, maxLines = 2, overflow = TextOverflow.Ellipsis)
            val facts = listOf(s.displayArtist, s.album, s.year.takeIf { it > 0 }?.toString() ?: "").filter { it.isNotEmpty() }
            Text(
                facts.joinToString(" · "),
                style = MaterialTheme.typography.bodyLarge,
                color = c.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }

        offer?.let { (os, pos) ->
            if (os.id == s.id) {
                Row(
                    Modifier
                        .padding(
                            horizontal = 16.dp,
                            vertical = 8.dp,
                        ).fillMaxWidth()
                        .background(c.primaryContainer, RoundedCornerShape(12.dp))
                        .padding(start = 16.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text("Continue from ${duration(pos)}?", Modifier.weight(1f), color = c.onPrimaryContainer)
                    TextButton({ pb.answerResume(false) }) { Text("No") }
                    TextButton({ pb.answerResume(true) }) { Text("Continue") }
                }
            }
        }

        val isFav = favs.any { it.song == s.id }
        val isLater = later.any { it.song == s.id }
        Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Tip(if (isFav) "Remove from Favorites" else "Add to Favorites") {
                IconButton({ scope.launch { app.store.mark(Store.FAV, s.id, !isFav) } }) {
                    Icon(
                        if (isFav) Icons.Heart else Icons.HeartOutline,
                        if (isFav) "Remove from Favorites" else "Add to Favorites",
                        tint = if (isFav) c.primary else c.onSurfaceVariant,
                    )
                }
            }
            Tip(if (isLater) "Remove from Listen Later" else "Listen later") {
                IconButton({ scope.launch { app.store.mark(Store.LATER, s.id, !isLater) } }) {
                    Icon(
                        if (isLater) Icons.Later else Icons.LaterOutline,
                        if (isLater) "Remove from Listen Later" else "Listen later",
                        tint = if (isLater) c.primary else c.onSurfaceVariant,
                    )
                }
            }
            Spacer(Modifier.weight(1f))
            Tip(if (lyrics) "Show the cover" else "Lyrics") {
                IconButton({ lyrics = !lyrics }) {
                    Icon(
                        Icons.Lyrics,
                        if (lyrics) "Show the cover" else "Lyrics",
                        tint = if (lyrics) c.primary else c.onSurfaceVariant.copy(alpha = if (s.hasLyrics) 1f else 0.38f),
                    )
                }
            }
            SpeedButton()
            SleepButton()
            val repeat = queue?.repeat ?: "off"
            val repeatLabel =
                when (repeat) {
                    "queue" -> "Repeat the queue"
                    "song" -> "Repeat this song"
                    else -> "Repeat: off"
                }
            Tip(repeatLabel) {
                IconButton({ pb.cycleRepeat() }) {
                    Icon(if (repeat == "song") Icons.RepeatOne else Icons.Repeat, repeatLabel, tint = if (repeat == "off") c.onSurfaceVariant else c.primary)
                }
            }
            Tip("Shuffle") {
                IconButton({ pb.toggleShuffle() }) {
                    Icon(Icons.Shuffle, "Shuffle", tint = if (queue?.shuffle == true) c.primary else c.onSurfaceVariant)
                }
            }
        }

        SeekBar(s.id, s.durationMs, playing)

        Row(
            Modifier.fillMaxWidth().padding(top = 4.dp, bottom = 24.dp),
            horizontalArrangement = Arrangement.SpaceEvenly,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Tip("Previous") { IconButton({ pb.player.seekToPrevious() }, Modifier.size(56.dp)) { Icon(Icons.Previous, "Previous", Modifier.size(36.dp)) } }
            Tip(if (playing) "Pause" else "Play") {
                Box(
                    Modifier
                        .size(76.dp)
                        .clip(CircleShape)
                        .background(c.primary)
                        .clickable { if (playing) pb.player.pause() else pb.player.play() },
                    contentAlignment = Alignment.Center,
                ) { Icon(if (playing) Icons.Pause else Icons.Play, if (playing) "Pause" else "Play", Modifier.size(36.dp), tint = c.onPrimary) }
            }
            Tip("Next") { IconButton({ pb.player.seekToNext() }, Modifier.size(56.dp)) { Icon(Icons.Next, "Next", Modifier.size(36.dp)) } }
        }
    }
}

@Composable
private fun SeekBar(
    songId: Long,
    songMs: Long,
    playing: Boolean,
) {
    val player = App.app.playback.player
    val c = MaterialTheme.colorScheme
    var position by remember { mutableLongStateOf(0L) }
    var length by remember { mutableLongStateOf(0L) }
    var dragging by remember { mutableStateOf(false) }
    var dragValue by remember { mutableFloatStateOf(0f) }
    LaunchedEffect(songId, playing) {
        while (true) {
            if (!dragging) position = player.currentPosition
            // Until the player has read the file, the catalogue knows the length.
            length = player.duration.takeIf { it > 0 } ?: songMs
            // Paused too, more slowly: a tap on a lyric line seeks without playing.
            delay(if (playing) 250 else 500)
        }
    }
    Column(Modifier.padding(horizontal = 24.dp)) {
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
        Row(Modifier.fillMaxWidth()) {
            val shown = if (dragging) (dragValue * length).toLong() else position
            Text(duration(shown), style = MaterialTheme.typography.bodySmall, color = c.onSurfaceVariant)
            Spacer(Modifier.weight(1f))
            Text("−" + duration((length - shown).coerceAtLeast(0)), style = MaterialTheme.typography.bodySmall, color = c.onSurfaceVariant)
        }
    }
}

/**
 * A thin seek bar as in the designs: Material's slider is a thick track
 * with a gap. Tap or drag anywhere on it; the 32 dp height is the touch area.
 */
@Composable
private fun Seek(
    fraction: Float,
    onDrag: (Float) -> Unit,
    onDone: () -> Unit,
) {
    val c = MaterialTheme.colorScheme
    var width by remember { mutableFloatStateOf(1f) }

    fun at(x: Float) = (x / width).coerceIn(0f, 1f)
    Canvas(
        Modifier
            .fillMaxWidth()
            .height(32.dp)
            .onSizeChanged { width = it.width.toFloat() }
            .pointerInput(Unit) {
                detectTapGestures {
                    onDrag(at(it.x))
                    onDone()
                }
            }.pointerInput(Unit) {
                detectHorizontalDragGestures(onDragEnd = onDone, onDragCancel = onDone) { change, _ -> onDrag(at(change.position.x)) }
            },
    ) {
        val y = size.height / 2
        val stroke = 4.dp.toPx()
        drawLine(c.outline, Offset(0f, y), Offset(size.width, y), stroke, StrokeCap.Round)
        val x = size.width * fraction.coerceIn(0f, 1f)
        drawLine(c.primary, Offset(0f, y), Offset(x, y), stroke, StrokeCap.Round)
        drawCircle(c.primary, 6.dp.toPx(), Offset(x, y))
    }
}
