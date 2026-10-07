package io.github.vivekg7.dhun.ui

import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import kotlin.math.roundToInt

/**
 * Drag a row by its handle to move it, in a list of 64 dp song rows: the
 * queue and a playlist. The row follows the finger; the move happens on
 * release, from the row's index to where it was dropped.
 */
class Reorder internal constructor(
    private val rowPx: Float,
    private val onMove: () -> (from: Int, to: Int) -> Unit,
) {
    var dragging by mutableIntStateOf(-1)
        private set
    private var offset by mutableFloatStateOf(0f)

    /** For the dragged row: on top, lifted, and under the finger. */
    fun row(i: Int): Modifier =
        Modifier.zIndex(if (i == dragging) 1f else 0f).graphicsLayer {
            if (i == dragging) {
                translationY = offset
                shadowElevation = 8f
            }
        }

    /** The handle at the start of row [i] of [count]. */
    @Composable
    fun Handle(
        i: Int,
        count: Int,
    ) = Tip("Drag to reorder") {
        Box(
            Modifier.width(44.dp).fillMaxHeight().pointerInput(i, count) {
                detectDragGestures(
                    onDragStart = {
                        dragging = i
                        offset = 0f
                    },
                    onDragEnd = {
                        val to = (dragging + (offset / rowPx).roundToInt()).coerceIn(0, count - 1)
                        if (dragging >= 0 && to != dragging) onMove()(dragging, to)
                        dragging = -1
                        offset = 0f
                    },
                    onDragCancel = {
                        dragging = -1
                        offset = 0f
                    },
                ) { change, drag ->
                    change.consume()
                    offset += drag.y
                }
            },
            contentAlignment = Alignment.Center,
        ) { Icon(Icons.Drag, "Drag to reorder", Modifier.size(20.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f)) }
    }
}

@Composable
fun rememberReorder(onMove: (from: Int, to: Int) -> Unit): Reorder {
    val rowPx = with(LocalDensity.current) { 64.dp.toPx() }
    val latest by rememberUpdatedState(onMove)
    return remember(rowPx) { Reorder(rowPx) { latest } }
}
