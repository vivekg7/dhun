package io.github.vivekg7.dhun.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Downloads
import io.github.vivekg7.dhun.data.Shelf
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.bool
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonPrimitive

/*
 * Audiobooks and podcasts, each a page of its own off the Playlists tab
 * (docs/plans/031_podcasts_and_audiobooks.md).
 */

/** What the user has heard of the spoken files: those marked played, and where the others were left. */
class Heard(
    val played: Set<Long>,
    private val left: Map<Long, Long>,
) {
    fun heardMs(s: Song) = if (s.id in played) s.durationMs else (left[s.id] ?: 0).coerceIn(0, s.durationMs)

    /** Where a book carries on: its first file not played, or from the start once all are. */
    fun next(shelf: Shelf) = shelf.items.firstOrNull { it.id !in played } ?: shelf.items.first()

    /** A book's progress, or its length before it is begun. */
    fun progress(shelf: Shelf): String {
        val total = shelf.items.sumOf { it.durationMs }
        val heard = shelf.items.sumOf(::heardMs)
        return when {
            shelf.items.all { it.id in played } -> "Finished"
            heard <= 0 || total <= 0 -> length(total)
            else -> "${heard * 100 / total}% · ${length(total - heard)} left"
        }
    }

    /** A file's line under its title: played, or how much is left of one begun. */
    fun state(s: Song): String? =
        when {
            s.id in played -> "Played"
            heardMs(s) > 0 -> "${length(s.durationMs - heardMs(s))} left"
            else -> null
        }
}

@Composable
fun rememberHeard(): Heard {
    val app = App.app
    val played by app.store.played.collectAsState(emptyList())
    val resumes by app.store.resumes.collectAsState(emptyList())
    return remember(played, resumes) { Heard(played.map { it.song }.toSet(), resumes.associate { it.song to it.positionMs }) }
}

/**
 * Playing from a book makes a queue of the whole book, named after it, from
 * the file tapped or the first one not played; its resume point does the
 * rest. Playing an episode makes a queue of the show's episodes from that
 * one on, those already played left out.
 */
fun playShelf(
    nav: Nav,
    shelf: Shelf,
    heard: Heard,
    from: Song? = null,
) {
    val start = from ?: heard.next(shelf)
    val source = (if (shelf.isBook) "book:" else "show:") + shelf.title
    if (shelf.isBook) {
        playList(nav, shelf.title, source, shelf.items, shelf.items.indexOf(start).coerceAtLeast(0))
    } else {
        val rest = shelf.items.drop(shelf.items.indexOf(start).coerceAtLeast(0)).filter { it == start || it.id !in heard.played }
        playList(nav, shelf.title, source, rest, 0)
    }
}

/** The page of every book, or of every show. */
@Composable
fun ShelvesScreen(
    books: Boolean,
    nav: Nav,
    onBack: () -> Unit,
) {
    val catalog by App.app.catalog.collectAsState()
    val heard = rememberHeard()
    val shelves = if (books) catalog.books else catalog.shows
    LazyColumn(Modifier.fillMaxSize()) {
        item {
            PageHeader(if (books) "Audiobooks" else "Podcasts", count(shelves.size, if (books) "book" else "show"), onBack, null, onShuffle = null)
        }
        items(shelves, key = { it.key }) { s ->
            val detail =
                if (books) {
                    listOf(s.author, heard.progress(s)).filter { it.isNotEmpty() }.joinToString(" · ")
                } else {
                    val new = s.items.count { it.id !in heard.played }
                    count(s.items.size, "episode") + if (new > 0) " · $new not played" else ""
                }
            ShelfRow(s, detail, pinned(Downloads.SHELF, s.key)) { nav.open(nav.tab, Page.ShelfPage(s.key)) }
        }
        if (shelves.isEmpty()) item { Empty("Nothing here yet") }
    }
}

/** A book or a show in a list: its cover, its title, and a line about it. */
@Composable
fun ShelfRow(
    s: Shelf,
    detail: String,
    downloaded: Boolean = false,
    onClick: () -> Unit,
) {
    val c = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .height(72.dp)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Art(s.cover ?: s.items.first(), 52.dp, RoundedCornerShape(6.dp))
        Column(Modifier.weight(1f).padding(start = 14.dp)) {
            Text(s.title, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(detail, style = MaterialTheme.typography.bodySmall, color = c.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (downloaded) Icon(Icons.Downloaded, "Downloaded", Modifier.padding(start = 8.dp).size(16.dp), tint = c.onSurfaceVariant)
    }
}

/**
 * A book, its files in reading order; or a show, its episodes newest
 * first, those played hidden unless asked for (the synced setting
 * [HIDE_PLAYED]).
 */
@Composable
fun ShelfScreen(
    key: String,
    nav: Nav,
    onBack: () -> Unit,
) {
    val app = App.app
    val catalog by app.catalog.collectAsState()
    val settings by app.store.settings.collectAsState(emptyMap())
    val heard = rememberHeard()
    val shelf = catalog.shelves[key] ?: return Empty("This is gone from the library", page = true)
    val hidePlayed = !shelf.isBook && settings.bool(HIDE_PLAYED, true)
    val rows = if (shelf.isBook) shelf.items else shelf.items.reversed().filter { !hidePlayed || it.id !in heard.played }
    val begun = shelf.items.any { heard.heardMs(it) > 0 }
    LazyColumn(Modifier.fillMaxSize()) {
        item {
            val facts =
                if (shelf.isBook) {
                    listOf(shelf.author, heard.progress(shelf))
                } else {
                    listOf(count(shelf.items.size, "episode"), "${shelf.items.count { it.id !in heard.played }} not played")
                }
            PageHeader(
                shelf.title,
                facts.filter { it.isNotEmpty() }.joinToString(" · "),
                onBack,
                { playShelf(nav, shelf, heard) },
                if (shelf.isBook) DownloadTarget(Downloads.SHELF, shelf.key, shelf.title, shelf.items) else null,
                songs = shelf.items,
                actions =
                    if (shelf.isBook) {
                        emptyList()
                    } else {
                        listOf(
                            (if (hidePlayed) "Show played episodes" else "Hide played episodes") to {
                                app.scope.launch { app.store.setting(HIDE_PLAYED, JsonPrimitive(!hidePlayed)) }
                                Unit
                            },
                        )
                    },
                playLabel = if (begun) "Continue" else "Play",
                onShuffle = null,
            )
        }
        itemsIndexed(rows, key = { _, s -> s.id }) { i, s ->
            // A book's files are often titled alike but for a part number at the end, cut off on a phone.
            val where =
                if (!shelf.isBook) {
                    s.date.take(10)
                } else if (rows.size > 1) {
                    "${i + 1} of ${rows.size}"
                } else {
                    ""
                }
            val detail = listOf(where, heard.state(s) ?: "").filter { it.isNotEmpty() }.joinToString(" · ")
            SongRow(s, onClick = { playShelf(nav, shelf, heard, s) }, menu = SongMenu(nav = nav), detail = detail.ifEmpty { s.displayArtist })
        }
        if (rows.isEmpty()) item { Empty("Every episode is played") }
    }
}

/** The chapter playing, under the title on Now playing; a tap lists them all. */
@Composable
fun ChapterLine(
    s: Song,
    playing: Boolean,
    onClick: () -> Unit,
) {
    val chapters = remember(s.chapters) { s.chapterList }
    if (chapters.isEmpty()) return
    val player = App.app.playback.player
    var pos by remember { mutableLongStateOf(player.currentPosition) }
    LaunchedEffect(s.id, playing) {
        while (true) {
            pos = player.currentPosition
            delay(1_000)
        }
    }
    val i = chapters.indexOfLast { it.startMs <= pos }.coerceAtLeast(0)
    Text(
        "Chapter ${i + 1} of ${chapters.size}" + chapters[i].name.let { if (it.isEmpty()) "" else " · $it" },
        Modifier.clickable(onClick = onClick).padding(vertical = 4.dp),
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.primary,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

/** An episode's or a book file's chapters, each a tap away, and its notes. */
@Composable
fun DetailsDialog(
    s: Song,
    onDismiss: () -> Unit,
) {
    val player = App.app.playback.player
    val chapters = remember(s.chapters) { s.chapterList }
    val now = chapters.indexOfLast { it.startMs <= player.currentPosition }
    val c = MaterialTheme.colorScheme
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(s.title, maxLines = 2, overflow = TextOverflow.Ellipsis) },
        text = {
            LazyColumn(Modifier.heightIn(max = 480.dp)) {
                if (chapters.isNotEmpty()) {
                    item {
                        Text("CHAPTERS", Modifier.padding(top = 8.dp, bottom = 4.dp), style = MaterialTheme.typography.labelSmall, color = c.secondary)
                    }
                }
                itemsIndexed(chapters) { i, ch ->
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .clickable {
                                player.seekTo(ch.startMs)
                                onDismiss()
                            }.padding(vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(duration(ch.startMs), Modifier.width(80.dp), style = MaterialTheme.typography.bodyMedium, color = c.onSurfaceVariant)
                        Text(
                            ch.name.ifEmpty { "Chapter ${i + 1}" },
                            style = MaterialTheme.typography.bodyLarge,
                            color = if (i == now) c.primary else c.onSurface,
                        )
                    }
                }
                if (s.notes.isNotEmpty()) {
                    item { Text("NOTES", Modifier.padding(top = 8.dp, bottom = 4.dp), style = MaterialTheme.typography.labelSmall, color = c.secondary) }
                    item { Text(s.notes, style = MaterialTheme.typography.bodyMedium) }
                }
            }
        },
        confirmButton = { TextButton(onDismiss) { Text("Close") } },
    )
}

/** "29 books", "1 show" */
private fun count(
    n: Int,
    what: String,
) = "$n $what" + if (n == 1) "" else "s"

/** Whether a show's page leaves out the episodes already played; synced, on by default. */
const val HIDE_PLAYED = "podcasts.hidePlayed"
