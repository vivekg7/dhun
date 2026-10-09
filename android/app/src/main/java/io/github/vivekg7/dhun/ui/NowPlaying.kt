package io.github.vivekg7.dhun.ui

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedContentTransitionScope
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.Crossfade
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
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
import androidx.compose.material3.ripple
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
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
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Covers
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.Store
import io.github.vivekg7.dhun.data.songIds
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@Composable
fun NowPlayingScreen(nav: Nav) {
    val app = App.app
    val pb = app.playback
    val song by pb.current.collectAsState()
    val playing by pb.playing.collectAsState()
    val waiting by pb.waiting.collectAsState()
    // Waiting counts as playing: the button pauses, rather than offering a play that is already wanted.
    val going = playing || waiting
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
        Empty("Nothing is playing.\nPick something from your library.", page = true)
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

        // A new song slides in from the side it comes from: the next from the right, the previous from the left.
        val ids = remember(queue?.songs) { queue?.let { songIds(it.songs) } ?: emptyList() }

        fun AnimatedContentTransitionScope<Song>.forward(): Int {
            val from = ids.indexOf(initialState.id)
            val to = ids.indexOf(targetState.id)
            // Off the end of a repeating queue to its start is still forward.
            return if (to >= from || (from == ids.lastIndex && to == 0)) 1 else -1
        }
        val gap = with(LocalDensity.current) { 24.dp.roundToPx() }
        val spec = tween<IntOffset>(Motion.LONG, easing = FastOutSlowInEasing)
        // The covers move side by side, as cards in a row; the words under them a shorter way, fading.
        val cards: AnimatedContentTransitionScope<Song>.() -> ContentTransform = {
            val sign = forward()
            (slideInHorizontally(spec) { sign * (it + gap) } togetherWith slideOutHorizontally(spec) { -sign * (it + gap) }) using SizeTransform(clip = false)
        }
        val slide: AnimatedContentTransitionScope<Song>.() -> ContentTransform = {
            val sign = forward()
            (slideInHorizontally(spec) { sign * it / 4 } + fadeIn(tween(Motion.LONG))) togetherWith
                (slideOutHorizontally(spec) { -sign * it / 4 } + fadeOut(tween(Motion.SHORT)))
        }

        // The cover takes what height is left, up to a square; a tap swaps it for the lyrics.
        BoxWithConstraints(Modifier.weight(1f).fillMaxWidth().padding(horizontal = 24.dp), contentAlignment = Alignment.Center) {
            val side = minOf(maxWidth, maxHeight)
            // The covers either side are fetched ahead, so Next and Previous slide in a cover, not an empty square.
            val sidePx = with(LocalDensity.current) { side.roundToPx() }
            LaunchedEffect(s.id, ids, sidePx) {
                val i = ids.indexOf(s.id)
                for (n in listOf(i + 1, i - 1)) ids.getOrNull(n)?.let { app.catalog.value.byId[it] }?.let { Covers.load(it, sidePx) }
            }
            AnimatedContent(s, contentAlignment = Alignment.Center, contentKey = { it.id }, transitionSpec = cards, label = "cover") { song ->
                Crossfade(lyrics, animationSpec = tween(Motion.MEDIUM), label = "lyrics") { showLyrics ->
                    if (showLyrics) {
                        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { LyricsView(song) { lyrics = false } }
                    } else {
                        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                            Art(
                                song,
                                side,
                                RoundedCornerShape(16.dp),
                                Modifier.clickable(interactionSource = null, indication = null) { lyrics = true },
                            )
                        }
                    }
                }
            }
        }

        AnimatedContent(s, contentKey = { it.id }, transitionSpec = slide, label = "title") { song ->
            Column(Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, top = 20.dp)) {
                Text(song.title, style = MaterialTheme.typography.titleLarge, maxLines = 2, overflow = TextOverflow.Ellipsis)
                val facts = listOf(song.displayArtist, song.album, song.year.takeIf { it > 0 }?.toString() ?: "").filter { it.isNotEmpty() }
                Text(
                    if (waiting) WAITING else facts.joinToString(" · "),
                    style = MaterialTheme.typography.bodyLarge,
                    color = if (waiting) c.primary else c.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }

        // The offer last made stays drawn while it folds away.
        val offerHere = offer?.takeIf { it.first.id == s.id }
        var lastOffer by remember { mutableStateOf(offerHere) }
        if (offerHere != null) lastOffer = offerHere
        AnimatedVisibility(offerHere != null, enter = expandVertically() + fadeIn(), exit = shrinkVertically() + fadeOut()) {
            lastOffer?.let { (_, pos) ->
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
            key(s.id) {
                Toggle(isFav, Icons.Heart, Icons.HeartOutline, if (isFav) "Remove from Favorites" else "Add to Favorites") {
                    scope.launch { app.store.mark(Store.FAV, s.id, !isFav) }
                }
                Toggle(isLater, Icons.Later, Icons.LaterOutline, if (isLater) "Remove from Listen Later" else "Listen later") {
                    scope.launch { app.store.mark(Store.LATER, s.id, !isLater) }
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
            Tip(if (going) "Pause" else "Play") {
                // Gives a little under the finger, and the icon turns from one to the other.
                val press = remember { MutableInteractionSource() }
                val pressed by press.collectIsPressedAsState()
                val scale by animateFloatAsState(if (pressed) 0.92f else 1f, spring(stiffness = Spring.StiffnessMedium), label = "press")
                Box(
                    Modifier
                        .size(76.dp)
                        .graphicsLayer {
                            scaleX = scale
                            scaleY = scale
                        }.clip(CircleShape)
                        .background(c.primary)
                        .clickable(interactionSource = press, indication = ripple()) { if (going) pb.player.pause() else pb.player.play() },
                    contentAlignment = Alignment.Center,
                ) {
                    AnimatedContent(
                        going,
                        transitionSpec = { (scaleIn(initialScale = 0.6f) + fadeIn()) togetherWith (scaleOut(targetScale = 0.6f) + fadeOut()) },
                        label = "play",
                    ) { g -> Icon(if (g) Icons.Pause else Icons.Play, if (g) "Pause" else "Play", Modifier.size(36.dp), tint = c.onPrimary) }
                }
            }
            Tip("Next") { IconButton({ pb.player.seekToNext() }, Modifier.size(56.dp)) { Icon(Icons.Next, "Next", Modifier.size(36.dp)) } }
        }
    }
}

/** On or off, with a little pop when turned on, as Favorites and Listen Later. */
@Composable
private fun Toggle(
    on: Boolean,
    onIcon: ImageVector,
    offIcon: ImageVector,
    label: String,
    onClick: () -> Unit,
) {
    val c = MaterialTheme.colorScheme
    val tint by animateColorAsState(if (on) c.primary else c.onSurfaceVariant, tween(Motion.MEDIUM), label = "tint")
    val pop = remember { Animatable(1f) }
    var first by remember { mutableStateOf(true) }
    LaunchedEffect(on) {
        // Only turning it on pops, not showing it on.
        if (on && !first) {
            pop.snapTo(0.7f)
            pop.animateTo(1f, spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = Spring.StiffnessMedium))
        }
        first = false
    }
    Tip(label) {
        IconButton(onClick) {
            Icon(
                if (on) onIcon else offIcon,
                label,
                Modifier.graphicsLayer {
                    scaleX = pop.value
                    scaleY = pop.value
                },
                tint = tint,
            )
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
fun Seek(
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

/** In place of the artist while the song cannot be fetched (docs/plans/019_networking_and_caching.md). */
const val WAITING = "Waiting for the network…"
