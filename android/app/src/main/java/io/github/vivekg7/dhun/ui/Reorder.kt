package io.github.vivekg7.dhun.ui

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.spring
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyItemScope
import androidx.compose.foundation.lazy.LazyListItemInfo
import androidx.compose.foundation.lazy.LazyListLayoutInfo
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * Drag a row of a lazy list by its handle to move it, as in Musicolet: the
 * row follows the finger, the rows it passes slide out of its way, and the
 * list scrolls when it is held near an edge. [shown] is the order on screen,
 * which changes while dragging; [onMove] is called once, on release, with
 * where the row started and where it was dropped, as indexes into the items.
 *
 * The new order stays on screen until the items given change, because the
 * move reaches them only after a database write: showing them at once would
 * snap the row back to where it started, then jump.
 */
class Reorder<T> internal constructor(
    private val list: LazyListState,
    private val scope: CoroutineScope,
    private val keyOf: (T) -> Any,
    private val edgePx: Float,
    private val lift: () -> Unit,
) {
    internal var items: List<T> = emptyList()
    internal var onMove: (from: Int, to: Int) -> Unit = { _, _ -> }

    // The items when the drag began, and their order since; null when not reordering.
    private var base by mutableStateOf<List<T>?>(null)
    private var order by mutableStateOf<List<T>>(emptyList())

    /** The row being dragged, or settling into place after it, and how far it is drawn from its place. */
    private var key by mutableStateOf<Any?>(null)
    private var offset by mutableFloatStateOf(0f)
    private var dragging by mutableStateOf(false)
    private var scrollSpeed by mutableFloatStateOf(0f)
    private var settle: Job? = null

    /** The layout a swap was made in: the next waits for a new one, so one move is never counted twice. */
    private var swappedIn: LazyListLayoutInfo? = null

    val shown: List<T>
        get() {
            val b = base ?: return items
            return if (b === items || (b.size == items.size && b.indices.all { keyOf(b[it]) == keyOf(items[it]) })) order else items
        }

    /**
     * For each row, inside `items { }`: the dragged one on top, lifted and
     * under the finger; the others slide to their new places.
     */
    fun LazyItemScope.row(key: Any): Modifier =
        if (key == this@Reorder.key) {
            Modifier.zIndex(1f).graphicsLayer {
                translationY = offset
                shadowElevation = if (dragging) 8.dp.toPx() else 0f
            }
        } else {
            Modifier.animateItem(fadeInSpec = null, fadeOutSpec = null, placementSpec = spring(stiffness = Spring.StiffnessMediumLow))
        }

    private fun start(k: Any) {
        if (dragging) return
        settle?.cancel()
        base = items
        order = items
        key = k
        offset = 0f
        dragging = true
        lift()
    }

    private fun drag(dy: Float) {
        offset += dy
        val at = itemOf(key) ?: return
        val view = list.layoutInfo
        // Within an edge band the list scrolls, faster the nearer the edge.
        val top = at.offset + offset - view.viewportStartOffset
        val bottom = view.viewportEndOffset - (at.offset + at.size + offset)
        scrollSpeed =
            when {
                top < edgePx && list.canScrollBackward -> -(edgePx - top.coerceAtLeast(0f)) / edgePx * MAX_SCROLL
                bottom < edgePx && list.canScrollForward -> (edgePx - bottom.coerceAtLeast(0f)) / edgePx * MAX_SCROLL
                else -> 0f
            }
        swap(at)
    }

    /** Moves the dragged row past the neighbour its middle is now over, keeping it under the finger. */
    private fun swap(at: LazyListItemInfo) {
        val view = list.layoutInfo
        if (view === swappedIn) return
        val mid = at.offset + offset + at.size / 2f
        val target =
            view.visibleItemsInfo.firstOrNull {
                // Past the neighbour's middle, so a row does not flick back and forth on its edge.
                it.key != at.key &&
                    if (it.index > at.index) mid > it.offset + it.size / 2f && mid < it.offset + it.size else mid < it.offset + it.size / 2f && mid > it.offset
            } ?: return
        val from = order.indexOfFirst { keyOf(it) == at.key }
        val to = order.indexOfFirst { keyOf(it) == target.key }
        // Not one of ours: a header above the rows, say.
        if (from < 0 || to < 0) return
        // A list keeps its first visible row in place; when that row is one of these two, hold the scroll still instead.
        val first = list.firstVisibleItemIndex
        if (at.index == first || target.index == first) list.requestScrollToItem(first, list.firstVisibleItemScrollOffset)
        order = order.toMutableList().apply { add(to, removeAt(from)) }
        swappedIn = view
        val moved = if (target.index > at.index) target.offset + target.size - at.size else target.offset
        offset += at.offset - moved
    }

    private fun drop() {
        if (!dragging) return
        dragging = false
        scrollSpeed = 0f
        val b = base
        val from = b?.indexOfFirst { keyOf(it) == key } ?: -1
        val to = order.indexOfFirst { keyOf(it) == key }
        if (from >= 0 && to >= 0 && from != to) onMove(from, to) else base = null
        // Settles into its place rather than jumping there.
        settle =
            scope.launch {
                animate(offset, 0f, animationSpec = spring(stiffness = Spring.StiffnessMediumLow)) { v, _ -> offset = v }
                key = null
            }
    }

    private fun itemOf(k: Any?) = list.layoutInfo.visibleItemsInfo.firstOrNull { it.key == k }

    /** Scrolls while the row is held near an edge, carrying the row with the finger. */
    internal suspend fun autoScroll() {
        while (true) {
            snapshotFlow { scrollSpeed }.first { it != 0f }
            while (scrollSpeed != 0f) {
                withFrameNanos { }
                val moved = list.scrollBy(scrollSpeed)
                if (moved == 0f) scrollSpeed = 0f
                offset += moved
                itemOf(key)?.let(::swap)
            }
        }
    }

    /** The handle at the start of the row with [key]. No tooltip: holding it before dragging would show one. */
    @Composable
    fun Handle(key: Any) =
        Box(
            Modifier.width(44.dp).fillMaxHeight().pointerInput(key) {
                detectDragGestures(
                    onDragStart = { start(key) },
                    onDragEnd = ::drop,
                    onDragCancel = ::drop,
                ) { change, d ->
                    change.consume()
                    drag(d.y)
                }
            },
            contentAlignment = Alignment.Center,
        ) { Icon(Icons.Drag, "Drag to reorder", Modifier.size(20.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f)) }

    private companion object {
        /** Pixels a frame at the very edge. */
        const val MAX_SCROLL = 24f
    }
}

@Composable
fun <T> rememberReorder(
    list: LazyListState,
    items: List<T>,
    key: (T) -> Any,
    onMove: (from: Int, to: Int) -> Unit,
): Reorder<T> {
    val scope = rememberCoroutineScope()
    val haptics = LocalHapticFeedback.current
    val edge = with(LocalDensity.current) { 64.dp.toPx() }
    val latestKey by rememberUpdatedState(key)
    val reorder =
        remember(list) { Reorder<T>(list, scope, { latestKey(it) }, edge) { haptics.performHapticFeedback(HapticFeedbackType.GestureThresholdActivate) } }
    reorder.items = items
    reorder.onMove = onMove
    LaunchedEffect(reorder) { reorder.autoScroll() }
    return reorder
}
