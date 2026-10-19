package io.github.vivekg7.dhun.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.ScrollIndicatorState
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.ScrollableState
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlin.math.roundToInt

/** A list shorter than this many screens scrolls well enough by hand, and gets no thumb. */
private const val LONG_SCREENS = 3

/** How long the thumb stays after the list stops, so it can be caught. */
private const val HIDE_AFTER_MS = 1500L

private val THUMB = 48.dp
private val TOUCH = 28.dp
private val BUBBLE = 64.dp

/** The letter a name is filed under in the bubble: its first letter, or # for a digit or a sign. */
fun initial(name: String): String {
    val c = name.trimStart().firstOrNull() ?: return "#"
    return if (c.isLetter()) c.uppercase() else "#"
}

/**
 * [content], a long list, with a thumb at its end edge that shows while it
 * scrolls; dragging the thumb jumps through the list. While dragging, the
 * bubble beside it shows [letter] of the row at the top, for a list in
 * name order (docs/plans/035_fast_scroller.md).
 */
@Composable
fun FastScroll(
    state: LazyListState,
    modifier: Modifier = Modifier,
    letter: ((Int) -> String?)? = null,
    content: @Composable () -> Unit,
) = FastScroll(
    state,
    { state.layoutInfo.totalItemsCount - state.layoutInfo.visibleItemsInfo.size },
    { state.firstVisibleItemIndex },
    { state.scrollToItem(it) },
    modifier,
    letter,
    content,
)

@Composable
fun FastScroll(
    state: LazyGridState,
    modifier: Modifier = Modifier,
    letter: ((Int) -> String?)? = null,
    content: @Composable () -> Unit,
) = FastScroll(
    state,
    { state.layoutInfo.totalItemsCount - state.layoutInfo.visibleItemsInfo.size },
    { state.firstVisibleItemIndex },
    { state.scrollToItem(it) },
    modifier,
    letter,
    content,
)

/**
 * The thumb follows the list's own estimate of where it is, but a drag
 * moves by item, not by pixel: scrolling 7,000 rows by pixels would lay
 * out every row in between.
 */
@Composable
private fun FastScroll(
    state: ScrollableState,
    range: () -> Int,
    first: () -> Int,
    jump: suspend (Int) -> Unit,
    modifier: Modifier,
    letter: ((Int) -> String?)?,
    content: @Composable () -> Unit,
) {
    var height by remember { mutableIntStateOf(0) }
    Box(modifier.onSizeChanged { height = it.height }) {
        content()
        val bar = state.scrollIndicatorState ?: return@Box
        if (bar.contentSize <= bar.viewportSize * LONG_SCREENS) return@Box
        Thumb(state, bar, height, range, first, jump, letter)
    }
}

@Composable
private fun BoxScope.Thumb(
    state: ScrollableState,
    bar: ScrollIndicatorState,
    height: Int,
    range: () -> Int,
    first: () -> Int,
    jump: suspend (Int) -> Unit,
    letter: ((Int) -> String?)?,
) {
    val density = LocalDensity.current
    val travel = (height - with(density) { THUMB.toPx() }).coerceAtLeast(1f)
    var dragging by remember { mutableStateOf(false) }
    var dragAt by remember { mutableFloatStateOf(0f) }
    var target by remember { mutableIntStateOf(-1) }
    var shown by remember { mutableStateOf(false) }
    val moving = state.isScrollInProgress
    LaunchedEffect(moving, dragging) {
        if (moving || dragging) {
            shown = true
        } else {
            delay(HIDE_AFTER_MS)
            shown = false
        }
    }
    // The newest place wins: a jump still running when the finger moves on is dropped.
    LaunchedEffect(Unit) { snapshotFlow { target }.collectLatest { if (it >= 0) jump(it) } }
    val alpha by animateFloatAsState(if (shown) 1f else 0f, label = "thumb")
    // While dragging the thumb stays under the finger; the list's estimate would make it twitch.
    val y = { if (dragging) dragAt else travel * (bar.scrollOffset.toFloat() / (bar.contentSize - bar.viewportSize)).coerceIn(0f, 1f) }
    val c = MaterialTheme.colorScheme
    Box(
        Modifier
            .align(Alignment.TopEnd)
            .offset { IntOffset(0, y().roundToInt()) }
            .size(TOUCH, THUMB)
            .graphicsLayer { this.alpha = alpha }
            .draggable(
                rememberDraggableState { d ->
                    dragAt = (dragAt + d).coerceIn(0f, travel)
                    target = (dragAt / travel * range()).roundToInt()
                },
                Orientation.Vertical,
                enabled = shown,
                onDragStarted = {
                    dragAt = y()
                    dragging = true
                },
                onDragStopped = {
                    dragging = false
                    target = -1
                },
            ),
    ) {
        Box(
            Modifier
                .align(Alignment.CenterEnd)
                .padding(end = 3.dp)
                .size(width = if (dragging) 8.dp else 4.dp, height = THUMB)
                .background(if (dragging) c.primary else c.onSurfaceVariant.copy(alpha = 0.6f), RoundedCornerShape(4.dp)),
        )
    }
    val name = if (dragging) letter?.invoke(first()) else null
    if (name != null) {
        Box(
            Modifier
                .align(Alignment.TopEnd)
                .offset { IntOffset(0, (y() + (THUMB - BUBBLE).toPx() / 2).roundToInt().coerceAtLeast(0)) }
                .padding(end = TOUCH + 8.dp)
                .size(BUBBLE)
                .alpha(alpha)
                .background(c.primaryContainer, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text(name, style = MaterialTheme.typography.headlineMedium, color = c.onPrimaryContainer)
        }
    }
}
