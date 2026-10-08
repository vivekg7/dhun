package io.github.vivekg7.dhun.ui

import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.DialogProperties
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.QueueRow
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.songIds
import io.github.vivekg7.dhun.play.SleepTimer
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * The Queues tab, laid out as Musicolet's (docs/plans/020_queue_screen.md):
 * the queue shown, numbered, in a box that opens the queue picker, with ✕
 * to remove it; a row of Play or Resume, Sort, where the queue is, Save and
 * more; the songs; and a search box at the bottom. Every queue can be
 * edited, not only the playing one.
 */
@Composable
fun QueuesScreen(nav: Nav) {
    val app = App.app
    val queues by app.playback.ordered.collectAsState()
    val activeId by app.playback.activeId.collectAsState()
    val current by app.playback.current.collectAsState()
    val playing by app.playback.playing.collectAsState()
    val waiting by app.playback.waiting.collectAsState()
    val sleep by app.playback.sleep.mode
        .collectAsState()
    val catalog by app.catalog.collectAsState()
    // A queue picked here stays shown, across tabs, until another queue starts playing.
    var picked by rememberSaveable { mutableStateOf<String?>(null) }
    var pickedWhile by rememberSaveable { mutableStateOf<String?>(null) }
    val pick = { id: String? ->
        picked = id
        pickedWhile = activeId
    }
    val shownId = picked.takeIf { pickedWhile == activeId }
    val shown = queues.firstOrNull { it.id == (shownId ?: activeId) } ?: queues.firstOrNull()
    if (shown == null) {
        Empty("No queues yet.\nPlaying anything from your library starts one.")
        return
    }
    val c = MaterialTheme.colorScheme
    val isActive = shown.id == activeId
    val songs = remember(shown.songs, catalog) { catalog.songsOf(songIds(shown.songs)) }
    val index = songs.indexOfFirst { it.id == (if (isActive) current?.id else shown.currentSong) }.coerceAtLeast(0)
    val scope = rememberCoroutineScope()
    val list = rememberLazyListState()

    var query by rememberSaveable(shown.id) { mutableStateOf("") }
    val rowPx = with(LocalDensity.current) { 64.dp.roundToPx() }
    val focus = LocalFocusManager.current

    /**
     * Back to the current song from anywhere in a long queue (Musicolet's
     * tap on the counter): the search is cleared, since the song may not
     * be among what it shows, and the song lands in the middle of the list.
     */
    val toCurrent: () -> Unit = {
        query = ""
        focus.clearFocus()
        scope.launch {
            snapshotFlow { list.layoutInfo.totalItemsCount }.first { it == songs.size }
            val visible = list.layoutInfo.viewportSize.height / rowPx
            list.animateScrollToItem((index - visible / 2).coerceAtLeast(0))
        }
    }
    val rows =
        remember(songs, query) {
            val q = query.trim()
            songs.withIndex().filter { (_, s) ->
                q.isEmpty() || s.title.contains(q, true) || s.displayArtist.contains(q, true) || s.album.contains(q, true)
            }
        }
    // Select multiple: null when not selecting.
    var selected by remember(shown.id) { mutableStateOf<Set<Long>?>(null) }
    BackHandler(selected != null) { selected = null }

    var picking by remember { mutableStateOf(false) }
    var removing by remember { mutableStateOf<QueueRow?>(null) }
    var renaming by remember { mutableStateOf<QueueRow?>(null) }
    var sorting by remember { mutableStateOf(false) }
    var saving by remember { mutableStateOf<List<Song>?>(null) }
    var queueing by remember { mutableStateOf<List<Song>?>(null) }

    Column(Modifier.fillMaxSize()) {
        Row(Modifier.padding(start = 8.dp, top = 8.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            val shape = RoundedCornerShape(10.dp)
            Row(
                Modifier
                    .weight(1f)
                    .height(48.dp)
                    .clip(shape)
                    .border(1.dp, c.outline, shape)
                    .clickable { picking = true }
                    .padding(start = 16.dp, end = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    "${queues.indexOf(shown) + 1}. ${shown.name}",
                    Modifier.weight(1f),
                    style = MaterialTheme.typography.titleMedium,
                    // Bold for the queue playing, as in Musicolet.
                    fontWeight = if (isActive) FontWeight.Bold else FontWeight.Normal,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Icon(Icons.ExpandMore, "Choose a queue", tint = c.onSurfaceVariant)
            }
            Tip("Remove this queue") { IconButton({ removing = shown }) { Icon(Icons.Close, "Remove this queue") } }
        }

        val sel = selected
        Row(Modifier.fillMaxWidth().height(56.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            if (sel == null) {
                if (isActive) {
                    val going = playing || waiting
                    Tip(if (going) "Pause" else "Play") {
                        IconButton({ if (going) app.playback.player.pause() else app.playback.player.play() }) {
                            Icon(if (going) Icons.Pause else Icons.Play, if (going) "Pause" else "Play")
                        }
                    }
                } else {
                    Tip("Resume this queue") { IconButton({ app.playback.switchTo(shown.id) }) { Icon(Icons.Play, "Resume this queue") } }
                }
                Tip("Sort this queue") { IconButton({ sorting = true }, enabled = songs.size > 1) { Icon(Icons.Sort, "Sort this queue") } }
                // Where the queue is; a tap scrolls back to its current song.
                Column(
                    Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .clip(RoundedCornerShape(8.dp))
                        .clickable(enabled = songs.isNotEmpty(), onClickLabel = "Scroll to the current song", onClick = toCurrent),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    Text(if (songs.isEmpty()) "0 / 0" else "${index + 1} / ${songs.size}", style = MaterialTheme.typography.labelLarge)
                    Text(
                        "${duration(songs.drop(index).sumOf { it.durationMs })} left of ${duration(songs.sumOf { it.durationMs })}",
                        style = MaterialTheme.typography.bodySmall,
                        color = c.onSurfaceVariant,
                    )
                }
                Tip("Save as playlist") { IconButton({ saving = songs }, enabled = songs.isNotEmpty()) { Icon(Icons.Save, "Save as playlist") } }
                Box {
                    var menu by remember { mutableStateOf(false) }
                    Tip("Queue options") { IconButton({ menu = true }) { Icon(Icons.More, "Queue options") } }
                    DropdownMenu(menu, { menu = false }) {
                        val close = { menu = false }
                        MenuItem("Select multiple", close) { selected = emptySet() }
                        MenuItem("Rename queue", close) { renaming = shown }
                    }
                }
            } else {
                val chosen = songs.filter { it.id in sel }
                val done = { selected = null }
                Tip("Done") { IconButton(done) { Icon(Icons.Close, "Done") } }
                Text("${sel.size} selected", Modifier.weight(1f).padding(start = 4.dp), style = MaterialTheme.typography.titleMedium)
                val all = rows.map { it.value.id }.toSet()
                TextButton({ selected = if (sel.containsAll(all)) sel - all else sel + all }) {
                    Text(if (sel.containsAll(all) && all.isNotEmpty()) "None" else "All")
                }
                Box {
                    var menu by remember { mutableStateOf(false) }
                    Tip("Options for selected songs") {
                        IconButton({ menu = true }, enabled = sel.isNotEmpty()) { Icon(Icons.More, "Options for selected songs") }
                    }
                    DropdownMenu(menu, { menu = false }) {
                        val close = { menu = false }
                        MenuItem("Play next", close) {
                            app.playback.playNext(chosen)
                            done()
                        }
                        MenuItem("Add to a queue…", close) { queueing = chosen }
                        MenuItem("Add to playlist…", close) { saving = chosen }
                        MenuItem("Remove from queue", close) {
                            app.playback.removeFrom(shown.id, sel)
                            done()
                        }
                    }
                }
            }
        }
        HorizontalDivider(color = c.outlineVariant)

        // To the current song when another queue is shown; coming back to the tab keeps the scroll.
        var scrolledFor by rememberSaveable { mutableStateOf<String?>(null) }
        LaunchedEffect(shown.id) {
            if (scrolledFor != shown.id) {
                scrolledFor = shown.id
                list.scrollToItem((index - 2).coerceAtLeast(0))
            }
        }
        val reorder = rememberReorder { from, to -> app.playback.moveIn(shown.id, from, to) }
        // Positions are the queue's own, so dragging waits until the search is cleared.
        val canDrag = query.isBlank() && sel == null
        LazyColumn(Modifier.weight(1f), state = list) {
            items(rows, key = { it.value.id }) { (i, s) ->
                val toggle = { selected = sel?.let { if (s.id in it) it - s.id else it + s.id } }
                val stopping = sleep is SleepTimer.Mode.AfterSong && (sleep as SleepTimer.Mode.AfterSong).song == s.id
                val extra =
                    buildList<Pair<String, () -> Unit>> {
                        add("Remove from queue" to { app.playback.removeFrom(shown.id, setOf(s.id)).let { } })
                        if (isActive) {
                            if (stopping) {
                                add("Don't stop after this song" to { app.playback.sleep.cancel() })
                            } else {
                                add("Stop after this song" to { app.playback.sleep.afterSong(s.id, s.title) })
                            }
                        }
                    }
                Box(reorder.row(i)) {
                    SongRow(
                        s,
                        onClick = {
                            when {
                                sel != null -> toggle()

                                isActive -> app.playback.playAt(i)

                                // Musicolet's way: a song of another queue starts that queue from it.
                                else -> app.playback.switchTo(shown.id).invokeOnCompletion { app.playback.playAt(i) }
                            }
                        },
                        current = i == index && songs.isNotEmpty(),
                        live = isActive,
                        selected = sel?.contains(s.id) == true,
                        onLongClick = { selected = (sel ?: emptySet()) + s.id },
                        leading =
                            when {
                                sel != null -> {
                                    { Box(Modifier.width(44.dp), contentAlignment = Alignment.Center) { Checkbox(s.id in sel, { toggle() }) } }
                                }

                                canDrag -> {
                                    { reorder.Handle(i, songs.size) }
                                }

                                else -> {
                                    null
                                }
                            },
                        menu = SongMenu(extra = extra, nav = nav),
                    )
                }
            }
        }
        SearchField(query, { query = it }, "Search in this queue…", bottom = true)
    }

    if (picking) {
        QueuePicker(
            queues,
            shownId = shown.id,
            activeId = activeId,
            onPick = {
                pick(it)
                picking = false
            },
            onRename = { renaming = it },
            onRemove = { removing = it },
            onDismiss = { picking = false },
        )
    }
    removing?.let { q ->
        AlertDialog(
            onDismissRequest = { removing = null },
            title = { Text("Remove “${q.name}”?") },
            text = { Text("The queue goes; its songs stay in your library.") },
            confirmButton = {
                TextButton({
                    app.playback.delete(q.id)
                    if (q.id == shown.id) pick(null)
                    removing = null
                }) { Text("Remove") }
            },
            dismissButton = { TextButton({ removing = null }) { Text("Cancel") } },
        )
    }
    renaming?.let { q ->
        NameDialog("Rename queue", q.name, "Rename") { name ->
            if (name != null) app.playback.rename(q.id, name)
            renaming = null
        }
    }
    if (sorting) {
        SortDialog({ sorting = false }) { sort ->
            app.playback.reorder(shown.id, sort.apply(songs).map { it.id })
            sorting = false
        }
    }
    saving?.let { s ->
        AddToPlaylistDialog(s, newName = if (s.size == songs.size) shown.name else "") {
            saving = null
            selected = null
        }
    }
    queueing?.let { s ->
        AddToQueueDialog(s) {
            queueing = null
            selected = null
        }
    }
}

/**
 * Musicolet's queue picker: every queue by number, the one shown marked,
 * ▶ on the one playing. Drag to reorder them, rename or remove one, or
 * remove all but the playing one.
 */
@Composable
private fun QueuePicker(
    queues: List<QueueRow>,
    shownId: String,
    activeId: String,
    onPick: (String) -> Unit,
    onRename: (QueueRow) -> Unit,
    onRemove: (QueueRow) -> Unit,
    onDismiss: () -> Unit,
) {
    val app = App.app
    val c = MaterialTheme.colorScheme
    val keep = activeId.takeIf { id -> queues.any { it.id == id } } ?: shownId
    var clearing by remember { mutableStateOf(false) }
    val reorder = rememberReorder(PICKER_ROW) { from, to -> app.playback.moveQueue(from, to) }
    AlertDialog(
        onDismissRequest = onDismiss,
        // Wide, as Musicolet's: the names are what you choose by.
        modifier = Modifier.padding(horizontal = 16.dp),
        properties = DialogProperties(usePlatformDefaultWidth = false),
        title = {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Queues, null)
                Spacer(Modifier.width(16.dp))
                Text("Queues")
            }
        },
        text = {
            // Eight and a half rows: the half shows there are more.
            LazyColumn(Modifier.heightIn(max = PICKER_ROW * 8.5f)) {
                itemsIndexed(queues, key = { _, q -> q.id }) { i, q ->
                    Row(
                        reorder
                            .row(i)
                            .fillMaxWidth()
                            .height(PICKER_ROW)
                            .clickable { onPick(q.id) },
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        reorder.Handle(i, queues.size)
                        RadioButton(q.id == shownId, null)
                        Text("${i + 1}", Modifier.padding(start = 12.dp).width(24.dp), style = MaterialTheme.typography.labelLarge, color = c.onSurfaceVariant)
                        if (q.id == activeId) Icon(Icons.Play, "Playing", Modifier.padding(end = 4.dp).size(18.dp), tint = c.primary)
                        Text(q.name, Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Tip("Rename queue") {
                            IconButton({ onRename(q) }) { Icon(Icons.Edit, "Rename queue", Modifier.size(20.dp), tint = c.onSurfaceVariant) }
                        }
                        Tip("Remove queue") {
                            IconButton({ onRemove(q) }) { Icon(Icons.Close, "Remove queue", Modifier.size(20.dp), tint = c.onSurfaceVariant) }
                        }
                    }
                }
            }
        },
        confirmButton = { TextButton(onDismiss) { Text("Close") } },
        dismissButton = { if (queues.size > 1) TextButton({ clearing = true }) { Text("Remove all others") } },
    )
    if (clearing) {
        val name = queues.firstOrNull { it.id == keep }?.name ?: ""
        AlertDialog(
            onDismissRequest = { clearing = false },
            title = { Text("Remove the other ${queues.size - 1} queues?") },
            text = { Text(if (keep == activeId) "Only “$name”, the queue playing, stays." else "Only “$name” stays.") },
            confirmButton = {
                TextButton({
                    app.playback.deleteOthers(keep)
                    clearing = false
                    onDismiss()
                }) { Text("Remove") }
            },
            dismissButton = { TextButton({ clearing = false }) { Text("Cancel") } },
        )
    }
}

private val PICKER_ROW = 56.dp

/** Add [songs] to one of the queues, by number as in the picker, or to a new one. */
@Composable
fun AddToQueueDialog(
    songs: List<Song>,
    onDismiss: () -> Unit,
) {
    val app = App.app
    val context = LocalContext.current
    val queues by app.playback.ordered.collectAsState()
    val activeId by app.playback.activeId.collectAsState()
    var naming by remember { mutableStateOf(false) }
    val note = { text: String -> Toast.makeText(context, text, Toast.LENGTH_SHORT).show() }
    if (naming) {
        NameDialog("New queue", songs.singleOrNull()?.title ?: "New queue", "Create") { name ->
            if (name != null) {
                app.playback.newQueue(name, songs)
                note("Made the queue “$name”")
            }
            onDismiss()
        }
        return
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (songs.size == 1) "Add “${songs[0].title}” to" else "Add ${songs.size} songs to") },
        text = {
            LazyColumn(Modifier.heightIn(max = 360.dp)) {
                item { DialogRow("New queue", icon = Icons.Add) { naming = true } }
                itemsIndexed(queues, key = { _, q -> q.id }) { i, q ->
                    DialogRow("${i + 1}. ${q.name}", icon = if (q.id == activeId) Icons.Play else Icons.Queues, detail = "${songIds(q.songs).size}") {
                        app.playback.addTo(q.id, songs)
                        note(if (songs.size == 1) "Added to “${q.name}”" else "Added ${songs.size} songs to “${q.name}”")
                        onDismiss()
                    }
                }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onDismiss) { Text("Cancel") } },
    )
}

/** How a queue can be sorted: Musicolet's Randomize and Reverse, then by what a song's tags say. */
private enum class QueueSort(
    val label: String,
    val apply: (List<Song>) -> List<Song>,
) {
    Randomize("Randomize", { it.shuffled() }),
    Reverse("Reverse", { it.reversed() }),
    Title("Title, A to Z", { s -> s.sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.title }) }),
    TitleDesc("Title, Z to A", { s -> Title.apply(s).reversed() }),
    Artist("Artist, A to Z", { s -> s.sortedWith(byArtist) }),
    ArtistDesc("Artist, Z to A", { s -> s.sortedWith(byArtist).reversed() }),
    Album("Album, A to Z", { s -> s.sortedWith(byAlbum) }),
    AlbumDesc("Album, Z to A", { s -> s.sortedWith(byAlbum).reversed() }),
    Folder("Folder and file name", { s -> s.sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.path }) }),
    Year("Year, oldest first", { s -> s.sortedBy { it.year } }),
    YearDesc("Year, newest first", { s -> s.sortedByDescending { it.year } }),
    Shortest("Shortest first", { s -> s.sortedBy { it.durationMs } }),
    Longest("Longest first", { s -> s.sortedByDescending { it.durationMs } }),
    Added("Recently added first", { s -> s.sortedByDescending { it.addedAt } }),
}

private val byAlbum: Comparator<Song> =
    compareBy<Song, String>(String.CASE_INSENSITIVE_ORDER) { it.album }.thenBy { it.disc }.thenBy { it.track }

private val byArtist: Comparator<Song> = compareBy<Song, String>(String.CASE_INSENSITIVE_ORDER) { it.displayArtist }.then(byAlbum)

@Composable
private fun SortDialog(
    onDismiss: () -> Unit,
    onPick: (QueueSort) -> Unit,
) = AlertDialog(
    onDismissRequest = onDismiss,
    title = { Text("Sort this queue") },
    text = {
        LazyColumn(Modifier.heightIn(max = 448.dp)) {
            items(QueueSort.entries) { s -> DialogRow(s.label) { onPick(s) } }
        }
    },
    confirmButton = {},
    dismissButton = { TextButton(onDismiss) { Text("Cancel") } },
)
