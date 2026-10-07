package io.github.vivekg7.dhun.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.HorizontalDivider
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
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.QueueRow
import io.github.vivekg7.dhun.data.songIds
import kotlin.math.roundToInt

/**
 * The queues, the signature feature (AGENTS.md): all of them as chips, the
 * one shown below. The playing queue can be reordered by dragging the handle.
 */
@Composable
fun QueuesScreen(nav: Nav) {
    val app = App.app
    val queues by app.playback.queues.collectAsState()
    val activeId by app.playback.activeId.collectAsState()
    val current by app.playback.current.collectAsState()
    val catalog by app.catalog.collectAsState()
    var shownId by remember { mutableStateOf<String?>(null) }
    val shown = queues.firstOrNull { it.id == (shownId ?: activeId) } ?: queues.firstOrNull()
    if (shown == null) {
        Empty("No queues yet.\nPlaying anything from your library starts one.")
        return
    }
    val isActive = shown.id == activeId
    val songs = remember(shown.songs, catalog) { catalog.songsOf(songIds(shown.songs)) }
    val index = songs.indexOfFirst { it.id == (if (isActive) current?.id else shown.currentSong) }.coerceAtLeast(0)
    val left = songs.drop(index).sumOf { it.durationMs }
    var editing by remember { mutableStateOf<QueueRow?>(null) }

    Column(Modifier.fillMaxSize()) {
        LazyRow(contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            items(queues, key = { it.id }) { q -> QueueChip(q, selected = q.id == shown.id, playing = q.id == activeId) { shownId = q.id } }
        }
        Row(Modifier.padding(start = 20.dp, end = 4.dp, bottom = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(shown.name, style = MaterialTheme.typography.headlineSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(
                    "Song ${index + 1} of ${songs.size} · ${duration(left)} left",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            if (!isActive) IconButton({ app.playback.switchTo(shown.id) }) { Icon(Icons.Play, "Play this queue") }
            IconButton({ if (isActive) app.playback.toggleShuffle() }, enabled = isActive) {
                Icon(Icons.Shuffle, "Shuffle", tint = if (shown.shuffle) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Box {
                var menu by remember { mutableStateOf(false) }
                IconButton({ menu = true }) { Icon(Icons.More, "Queue options") }
                DropdownMenu(menu, { menu = false }) {
                    val close = { menu = false }
                    MenuItem("Rename", close) { editing = shown }
                    MenuItem("Remove queue", close) {
                        app.playback.delete(shown.id)
                        shownId = null
                    }
                }
            }
        }
        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)

        val list = rememberLazyListState()
        LaunchedEffect(shown.id) { if (index > 2) list.scrollToItem(index - 2) }
        var dragging by remember { mutableIntStateOf(-1) }
        var offset by remember { mutableFloatStateOf(0f) }
        val rowPx = with(LocalDensity.current) { 64.dp.toPx() }
        LazyColumn(Modifier.fillMaxSize(), state = list) {
            itemsIndexed(songs, key = { _, s -> s.id }) { i, s ->
                val handle = @Composable {
                    Box(
                        Modifier.width(44.dp).fillMaxHeight().pointerInput(isActive, songs.size) {
                            if (!isActive) return@pointerInput
                            detectDragGestures(
                                onDragStart = {
                                    dragging = i
                                    offset = 0f
                                },
                                onDragEnd = {
                                    val to = (dragging + (offset / rowPx).roundToInt()).coerceIn(songs.indices)
                                    if (dragging >= 0 && to != dragging) app.playback.move(dragging, to)
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
                Box(
                    Modifier
                        .zIndex(
                            if (i ==
                                dragging
                            ) {
                                1f
                            } else {
                                0f
                            },
                        ).graphicsLayer {
                            if (i == dragging) translationY = offset
                            if (i == dragging) shadowElevation = 8f
                        },
                ) {
                    SongRow(
                        s,
                        onClick = { if (isActive) app.playback.playAt(i) else app.playback.switchTo(shown.id).invokeOnCompletion { app.playback.playAt(i) } },
                        current = i == index,
                        leading = if (isActive) handle else null,
                        menu =
                            SongMenu(
                                extra =
                                    if (isActive) {
                                        listOf(
                                            "Remove from queue" to {
                                                app.playback.removeAt(i)
                                                Unit
                                            },
                                        )
                                    } else {
                                        emptyList()
                                    },
                                nav = nav,
                            ),
                    )
                }
            }
        }
    }
    editing?.let { q ->
        RenameDialog(q.name, onDone = { name ->
            if (name != null &&
                name.isNotBlank()
            ) {
                app.playback.rename(q.id, name.trim())
            }
            ; editing = null
        })
    }
}

@Composable
private fun QueueChip(
    q: QueueRow,
    selected: Boolean,
    playing: Boolean,
    onClick: () -> Unit,
) {
    val c = MaterialTheme.colorScheme
    Row(
        Modifier
            .height(34.dp)
            .background(if (selected) c.primaryContainer else c.surface, RoundedCornerShape(17.dp))
            .border(BorderStroke(1.dp, if (selected) c.primary else c.outline), RoundedCornerShape(17.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (playing) {
            Icon(Icons.Play, null, Modifier.size(14.dp), tint = c.primary)
            Spacer(Modifier.width(4.dp))
        }
        Text(q.name, style = MaterialTheme.typography.bodyMedium, color = if (selected) c.onPrimaryContainer else c.onSurfaceVariant, maxLines = 1)
    }
}

@Composable
fun RenameDialog(
    name: String,
    onDone: (String?) -> Unit,
) {
    var text by remember { mutableStateOf(name) }
    AlertDialog(
        onDismissRequest = { onDone(null) },
        title = { Text("Rename queue") },
        text = { OutlinedTextField(text, { text = it }, singleLine = true) },
        confirmButton = { TextButton({ onDone(text) }) { Text("Rename") } },
        dismissButton = { TextButton({ onDone(null) }) { Text("Cancel") } },
    )
}
