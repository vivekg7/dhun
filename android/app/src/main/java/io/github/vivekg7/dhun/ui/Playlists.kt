package io.github.vivekg7.dhun.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Catalog
import io.github.vivekg7.dhun.data.Downloads
import io.github.vivekg7.dhun.data.PlayStat
import io.github.vivekg7.dhun.data.Playlist
import io.github.vivekg7.dhun.data.SearchIndex
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.Store
import io.github.vivekg7.dhun.data.songIds
import kotlinx.coroutines.launch

/**
 * Favorites and Listen Later, then the views computed from what the app
 * already syncs (docs/plans/010_special_playlists.md).
 */
enum class ListKind(
    val label: String,
    val icon: ImageVector,
    val source: String,
) {
    Favorites("Favorites", Icons.Heart, "favorites"),
    ListenLater("Listen Later", Icons.Later, "listen_later"),
    Continue("Continue listening", Icons.Continue, "continue"),
    RecentlyAdded("Recently added", Icons.RecentlyAdded, "recently_added"),
    RecentlyPlayed("Recently played", Icons.History, "recently_played"),
    MostPlayed("Most played", Icons.MostPlayed, "most_played"),
    NotLately("Not played lately", Icons.Hourglass, "not_played_lately"),
}

private const val VIEW_SIZE = Catalog.VIEW_SIZE
private const val DAY = 86_400_000L

/**
 * Not played lately: played at least 3 times (with the 50% rule of plan
 * 008), and not in the last 90 days. Favourites you have drifted away from.
 */
private const val LATELY_MIN_PLAYS = 3
private const val LATELY_DAYS = 90

/**
 * All the lists, each computed when first asked for and kept while what it
 * reads is unchanged: a resume point written every 30 s of a long file
 * redoes the marks' lists, not the play counts' (docs/plans/036_fast_at_100k.md).
 */
@Composable
fun rememberLists(): (ListKind) -> List<Song> {
    val app = App.app
    val catalog by app.catalog.collectAsState()
    val favs by app.store.favorites.collectAsState()
    val later by app.store.listenLater.collectAsState()
    val resumes by app.store.resumes.collectAsState()
    val stats by app.store.playStats.collectAsState()
    val marked = remember(catalog, favs, later, resumes) { HashMap<ListKind, List<Song>>() }
    val played = remember(catalog, stats) { HashMap<ListKind, List<Song>>() }
    return { k ->
        when (k) {
            ListKind.Favorites -> marked.getOrPut(k) { catalog.songsOf(favs.map { it.song }) }
            ListKind.ListenLater -> marked.getOrPut(k) { catalog.songsOf(later.map { it.song }) }
            ListKind.Continue -> marked.getOrPut(k) { catalog.songsOf(resumes.map { it.song }) }
            ListKind.RecentlyAdded -> catalog.recentlyAdded
            else -> played.getOrPut(k) { fromStats(k, catalog, stats) }
        }
    }
}

private fun fromStats(
    kind: ListKind,
    c: Catalog,
    stats: List<PlayStat>,
): List<Song> {
    val cutoff = System.currentTimeMillis() - LATELY_DAYS * DAY
    val picked =
        when (kind) {
            ListKind.RecentlyPlayed -> stats.filter { it.lastPlayedAt > 0 }.sortedByDescending { it.lastPlayedAt }
            ListKind.MostPlayed -> stats.filter { it.count > 0 }.sortedByDescending { it.count }
            ListKind.NotLately -> stats.filter { it.count >= LATELY_MIN_PLAYS && it.lastPlayedAt < cutoff }.sortedByDescending { it.count }
            else -> error("$kind is not a played list")
        }
    return c.songsOf(picked.map { it.song }).take(VIEW_SIZE)
}

@Composable
fun PlaylistsScreen(nav: Nav) {
    val app = App.app
    val playlists by app.store.playlists.collectAsState()
    val catalog by app.catalog.collectAsState()
    var query by rememberSaveable { mutableStateOf("") }
    val q = query.trim()
    val lists = rememberLists()
    val context = LocalContext.current
    var naming by remember { mutableStateOf(false) }
    if (naming) {
        NameDialog("New playlist", "", "Create") { name ->
            if (name != null) savePlaylist(context, name, emptyList())
            naming = false
        }
    }
    val shown = rememberFound(playlists, query) { SearchIndex.names(it, "playlist") { p -> p.name } }.map { it.value }
    val list = rememberLazyListState()
    // The playlists, in name order, are the last rows, after the cards and the automatic lists.
    val letter = { i: Int -> shown.getOrNull(i - list.layoutInfo.totalItemsCount + shown.size)?.name?.let(::initial) }
    FastScroll(list, Modifier.fillMaxSize(), letter) {
        LazyColumn(Modifier.fillMaxSize(), list) {
            item { SearchField(query, { query = it }, "Search playlists…") }
            if (q.isEmpty()) {
                item {
                    Row(Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        for (k in listOf(ListKind.Favorites, ListKind.ListenLater)) {
                            PinnedCard(k.icon, k.label, summary(lists(k)), Modifier.weight(1f)) { nav.open(Tab.Playlists, Page.ListPage(k)) }
                        }
                    }
                }
                // Each only once its collection has something in it (docs/plans/031_podcasts_and_audiobooks.md).
                if (catalog.books.isNotEmpty() || catalog.shows.isNotEmpty()) {
                    item {
                        Row(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            if (catalog.books.isNotEmpty()) {
                                PinnedCard(
                                    Icons.Book,
                                    "Audiobooks",
                                    count(catalog.books.size, "book"),
                                    Modifier.weight(1f),
                                ) {
                                    nav.open(Tab.Playlists, Page.ShelvesPage(books = true))
                                }
                            }
                            if (catalog.shows.isNotEmpty()) {
                                PinnedCard(
                                    Icons.Podcast,
                                    "Podcasts",
                                    count(catalog.shows.size, "show"),
                                    Modifier.weight(1f),
                                ) {
                                    nav.open(Tab.Playlists, Page.ShelvesPage(books = false))
                                }
                            }
                        }
                    }
                }
                item { DownloadsCard { nav.open(Tab.Playlists, Page.DownloadsPage) } }
                item { SectionLabel("Automatic") }
                items(ListKind.entries.drop(2), key = { it.name }) { k ->
                    NameRow(k.icon, k.label, "${lists(k).size}") { nav.open(Tab.Playlists, Page.ListPage(k)) }
                }
                item { SectionLabel("Playlists") }
                item { NameRow(Icons.Add, "New playlist", "") { naming = true } }
            }
            items(shown, key = { it.id }) { p ->
                val count = catalog.shownCount(p)
                NameRow(Icons.Playlist, p.name, if (p.shared) "$count · shared" else "$count", pinned(Downloads.PLAYLIST, p.id.toString())) {
                    nav.open(Tab.Playlists, Page.PlaylistPage(p.id))
                }
            }
        }
    }
}

@Composable
private fun PinnedCard(
    icon: ImageVector,
    label: String,
    detail: String,
    modifier: Modifier,
    onClick: () -> Unit,
) {
    val c = MaterialTheme.colorScheme
    // As the Downloads card under it: the three are one kind of thing.
    Column(
        modifier
            .clip(RoundedCornerShape(16.dp))
            .background(c.surfaceContainer)
            .clickable(onClick = onClick)
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Icon(icon, null, Modifier.size(26.dp), tint = c.secondary)
        Column {
            Text(label, style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold))
            Text(detail, style = MaterialTheme.typography.bodySmall, color = c.onSurfaceVariant)
        }
    }
}

/** Under Favorites and Listen Later: what is on the phone, and what the downloader is doing. */
@Composable
private fun DownloadsCard(onClick: () -> Unit) {
    val status by App.app.downloads.status
        .collectAsState()
    val c = MaterialTheme.colorScheme
    Row(
        Modifier
            .padding(horizontal = 16.dp)
            .fillMaxWidth()
            .clip(RoundedCornerShape(16.dp))
            .background(c.surfaceContainer)
            .clickable(onClick = onClick)
            .padding(16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Download, null, Modifier.size(26.dp), tint = c.secondary)
        Column(Modifier.padding(start = 16.dp)) {
            Text("Downloads", style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold))
            Text(describe(status), style = MaterialTheme.typography.bodySmall, color = c.onSurfaceVariant)
        }
    }
}

@Composable
fun ListScreen(
    kind: ListKind,
    nav: Nav,
    onBack: () -> Unit,
) {
    val songs = rememberLists()(kind)
    // Favorites and Listen Later can be downloaded; the automatic views change after every listen (docs/plans/012_downloads.md).
    val markKind =
        when (kind) {
            ListKind.Favorites -> Store.FAV
            ListKind.ListenLater -> Store.LATER
            else -> null
        }
    SongList(kind.label, kind.source, songs, nav, summary(songs), onBack, markKind?.let { DownloadTarget(Downloads.LIST, it, kind.label, songs) })
}

/** How many of [p]'s songs a playlist page shows: those the catalogue has. */
fun Catalog.shownCount(p: Playlist) = songIds(p.songs).count { it != 0L && it in byId }

/**
 * A playlist. Your own can be edited here: drag to reorder, remove a song,
 * rename, delete (docs/plans/013_playlist_editing.md); shared ones are
 * read-only in the app.
 */
@Composable
fun PlaylistScreen(
    id: Long,
    nav: Nav,
    onBack: () -> Unit,
) {
    val app = App.app
    val playlists by app.store.playlists.collectAsState()
    val catalog by app.catalog.collectAsState()
    // One made on this phone is replaced by the server's copy after a sync.
    val p = playlists.firstOrNull { it.id == id } ?: playlists.firstOrNull { it.id == app.sync.replaced[id] } ?: return Empty("", page = true)
    // 0 is an entry the server could not match to a song; it is kept in the
    // file but not shown. Rows remember their place in the file, for edits.
    val rows = remember(p.songs, catalog) { songIds(p.songs).withIndex().mapNotNull { (i, sid) -> catalog.byId[sid]?.let { i to it } } }
    val songs = rows.map { it.second }
    val searchable = rows.size > SEARCH_OVER
    var query by rememberSaveable { mutableStateOf("") }
    val q = if (searchable) query else ""
    // Rows found keep their place in the playlist, so a song found plays the whole playlist from it.
    val hits = rememberFound(songs, q) { SearchIndex.songs(it) }.map { IndexedValue(it.index, rows[it.index]) }
    val source = "playlist:${p.id}"
    val editable = p.editable()
    var renaming by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf(false) }
    val list = rememberLazyListState()
    val reorder =
        rememberReorder(
            list,
            hits,
            { (_, r) -> "${r.first}/${r.second.id}" },
        ) { from, to -> app.scope.launch { app.store.moveInPlaylist(p, hits[from].value.first, hits[to].value.first) } }
    // Moving among the songs found would put them in the wrong places, so dragging waits until the search is cleared.
    val canDrag = editable && q.isBlank()
    Column(Modifier.fillMaxSize()) {
        FastScroll(list, Modifier.weight(1f)) {
            LazyColumn(Modifier.fillMaxSize(), state = list) {
                item {
                    PageHeader(
                        p.name,
                        summary(songs) + if (p.shared) " · shared" else "",
                        onBack,
                        { playList(nav, p.name, source, songs, 0) },
                        DownloadTarget(Downloads.PLAYLIST, p.id.toString(), p.name, songs),
                        songs = songs,
                        actions = if (editable) listOf("Rename" to { renaming = true }, "Delete playlist" to { deleting = true }) else emptyList(),
                    ) { shuffleList(nav, p.name, source, songs) }
                }
                items(reorder.shown, key = { (_, r) -> "${r.first}/${r.second.id}" }, contentType = { "song" }) { (i, r) ->
                    val (raw, s) = r
                    val remove = listOf("Remove from playlist" to { app.scope.launch { app.store.removeFromPlaylist(p, raw) }.let { } })
                    Box(with(reorder) { row("$raw/${s.id}") }) {
                        SongRow(
                            s,
                            onClick = { playList(nav, p.name, source, songs, i) },
                            leading = if (canDrag) ({ reorder.Handle("$raw/${s.id}") }) else null,
                            menu = SongMenu(extra = if (editable) remove else emptyList(), nav = nav),
                        )
                    }
                }
                if (rows.isEmpty()) item { Empty(if (editable) "Empty. Add songs with “Add to playlist” in any song's menu." else "Nothing here yet") }
                if (rows.isNotEmpty() && hits.isEmpty()) item { Empty("Nothing here matches “${query.trim()}”") }
            }
        }
        if (searchable) SearchField(query, { query = it }, "Search in this playlist…", bottom = true)
    }
    if (renaming) {
        NameDialog("Rename playlist", p.name, "Rename") { name ->
            if (name != null && name != p.name) app.scope.launch { app.store.renamePlaylist(p, name) }
            renaming = false
        }
    }
    if (deleting) {
        ConfirmDialog(
            "Delete “${p.name}”?",
            "The songs stay in the library. The server keeps a copy of the playlist file in its data folder.",
            "Delete",
            "Keep",
            { deleting = false },
        ) {
            app.scope.launch { app.store.deletePlaylist(p) }
            onBack()
        }
    }
}
