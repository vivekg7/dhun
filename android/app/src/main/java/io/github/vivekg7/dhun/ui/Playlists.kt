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
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Catalog
import io.github.vivekg7.dhun.data.Downloads
import io.github.vivekg7.dhun.data.Mark
import io.github.vivekg7.dhun.data.PlayStat
import io.github.vivekg7.dhun.data.Resume
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

private const val VIEW_SIZE = 100
private const val DAY = 86_400_000L

/**
 * Not played lately: played at least 3 times (with the 50% rule of plan
 * 008), and not in the last 90 days. Favourites you have drifted away from.
 */
private const val LATELY_MIN_PLAYS = 3
private const val LATELY_DAYS = 90

@Composable
fun listSongs(kind: ListKind): List<Song> = rememberLists()(kind)

/** All the lists from one read of the data they come from, computed when first asked for. */
@Composable
fun rememberLists(): (ListKind) -> List<Song> {
    val app = App.app
    val catalog by app.catalog.collectAsState()
    val favs by app.store.favorites.collectAsState(emptyList())
    val later by app.store.listenLater.collectAsState(emptyList())
    val resumes by app.store.resumes.collectAsState(emptyList())
    val stats by app.store.playStats.collectAsState(emptyList())
    return remember(catalog, favs, later, resumes, stats) {
        val cache = HashMap<ListKind, List<Song>>()
        val lists: (ListKind) -> List<Song> = { k -> cache.getOrPut(k) { compute(k, catalog, favs, later, resumes, stats) } }
        lists
    }
}

private fun compute(
    kind: ListKind,
    c: Catalog,
    favs: List<Mark>,
    later: List<Mark>,
    resumes: List<Resume>,
    stats: List<PlayStat>,
): List<Song> =
    when (kind) {
        ListKind.Favorites -> {
            c.songsOf(favs.map { it.song })
        }

        ListKind.ListenLater -> {
            c.songsOf(later.map { it.song })
        }

        ListKind.Continue -> {
            c.songsOf(resumes.map { it.song })
        }

        ListKind.RecentlyAdded -> {
            c.songs.sortedByDescending { it.addedAt }.take(VIEW_SIZE)
        }

        ListKind.RecentlyPlayed -> {
            c.songsOf(stats.filter { it.lastPlayedAt > 0 }.sortedByDescending { it.lastPlayedAt }.map { it.song }).take(VIEW_SIZE)
        }

        ListKind.MostPlayed -> {
            c.songsOf(stats.filter { it.count > 0 }.sortedByDescending { it.count }.map { it.song }).take(VIEW_SIZE)
        }

        ListKind.NotLately -> {
            val cutoff = System.currentTimeMillis() - LATELY_DAYS * DAY
            c.songsOf(stats.filter { it.count >= LATELY_MIN_PLAYS && it.lastPlayedAt < cutoff }.sortedByDescending { it.count }.map { it.song }).take(VIEW_SIZE)
        }
    }

@Composable
fun PlaylistsScreen(nav: Nav) {
    val app = App.app
    val playlists by app.store.playlists.collectAsState(emptyList())
    val catalog by app.catalog.collectAsState()
    var query by rememberSaveable { mutableStateOf("") }
    val q = Catalog.fold(query)
    val lists = rememberLists()
    val context = androidx.compose.ui.platform.LocalContext.current
    var naming by remember { mutableStateOf(false) }
    if (naming) {
        NameDialog("New playlist", "", "Create") { name ->
            if (name != null) savePlaylist(context, name, emptyList())
            naming = false
        }
    }
    LazyColumn(Modifier.fillMaxSize()) {
        item { SearchField(query, { query = it }, "Search playlists…") }
        if (q.isEmpty()) {
            item {
                Row(Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    PinnedCard(
                        ListKind.Favorites,
                        lists(ListKind.Favorites),
                        filled = true,
                        Modifier.weight(1f),
                    ) { nav.open(Tab.Playlists, Page.ListPage(ListKind.Favorites)) }
                    PinnedCard(ListKind.ListenLater, lists(ListKind.ListenLater), filled = false, Modifier.weight(1f)) {
                        nav.open(Tab.Playlists, Page.ListPage(ListKind.ListenLater))
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
        val shown = playlists.filter { q.isEmpty() || Catalog.fold(it.name).contains(q) }
        items(shown, key = { it.id }) { p ->
            val count = songIds(p.songs).count { it != 0L && catalog.byId.containsKey(it) }
            NameRow(Icons.Playlist, p.name, if (p.shared) "$count · shared" else "$count", pinned(Downloads.PLAYLIST, p.id.toString())) {
                nav.open(Tab.Playlists, Page.PlaylistPage(p.id))
            }
        }
    }
}

@Composable
private fun PinnedCard(
    kind: ListKind,
    songs: List<Song>,
    filled: Boolean,
    modifier: Modifier,
    onClick: () -> Unit,
) {
    val c = MaterialTheme.colorScheme
    val fg = if (filled) c.onPrimary else c.onSurface
    Column(
        modifier
            .clip(RoundedCornerShape(16.dp))
            .background(if (filled) c.primary else c.surfaceContainer)
            .clickable(onClick = onClick)
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Icon(kind.icon, null, Modifier.size(26.dp), tint = if (filled) fg else c.secondary)
        Column {
            Text(kind.label, style = MaterialTheme.typography.titleMedium.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Bold), color = fg)
            Text(summary(songs), style = MaterialTheme.typography.bodySmall, color = if (filled) fg.copy(alpha = 0.85f) else c.onSurfaceVariant)
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
        verticalAlignment = androidx.compose.ui.Alignment.CenterVertically,
    ) {
        Icon(Icons.Download, null, Modifier.size(26.dp), tint = c.secondary)
        Column(Modifier.padding(start = 16.dp)) {
            Text("Downloads", style = MaterialTheme.typography.titleMedium.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Bold))
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
    val songs = listSongs(kind)
    SongList(kind.label, kind.source, songs, nav) {
        // Favorites and Listen Later can be downloaded; the automatic views change after every listen (docs/plans/012_downloads.md).
        val markKind =
            when (kind) {
                ListKind.Favorites -> Store.FAV
                ListKind.ListenLater -> Store.LATER
                else -> null
            }
        PageHeader(
            kind.label,
            summary(songs),
            onBack,
            { playList(kind.label, kind.source, songs, 0) },
            markKind?.let { DownloadTarget(Downloads.LIST, it, kind.label, songs) },
            songs = songs,
        ) { shuffleList(kind.label, kind.source, songs) }
    }
}

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
    val playlists by app.store.playlists.collectAsState(emptyList())
    val catalog by app.catalog.collectAsState()
    // One made on this phone is replaced by the server's copy after a sync.
    val p = playlists.firstOrNull { it.id == id } ?: playlists.firstOrNull { it.id == app.sync.replaced[id] } ?: return Empty("")
    // 0 is an entry the server could not match to a song; it is kept in the
    // file but not shown. Rows remember their place in the file, for edits.
    val rows = remember(p.songs, catalog) { songIds(p.songs).withIndex().mapNotNull { (i, sid) -> catalog.byId[sid]?.let { i to it } } }
    val songs = rows.map { it.second }
    val source = "playlist:${p.id}"
    val editable = p.editable()
    var renaming by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf(false) }
    val reorder = rememberReorder { from, to -> app.scope.launch { app.store.moveInPlaylist(p, rows[from].first, rows[to].first) } }
    LazyColumn(Modifier.fillMaxSize()) {
        item {
            PageHeader(
                p.name,
                summary(songs) + if (p.shared) " · shared" else "",
                onBack,
                { playList(p.name, source, songs, 0) },
                DownloadTarget(Downloads.PLAYLIST, p.id.toString(), p.name, songs),
                songs = songs,
                actions = if (editable) listOf("Rename" to { renaming = true }, "Delete playlist" to { deleting = true }) else emptyList(),
            ) { shuffleList(p.name, source, songs) }
        }
        itemsIndexed(rows, key = { _, (raw, s) -> "$raw/${s.id}" }) { i, (raw, s) ->
            val remove = listOf("Remove from playlist" to { app.scope.launch { app.store.removeFromPlaylist(p, raw) }.let { } })
            Box(reorder.row(i)) {
                SongRow(
                    s,
                    onClick = { playList(p.name, source, songs, i) },
                    leading = if (editable) ({ reorder.Handle(i, rows.size) }) else null,
                    menu = SongMenu(extra = if (editable) remove else emptyList(), nav = nav),
                )
            }
        }
        if (rows.isEmpty()) item { Empty(if (editable) "Empty. Add songs with “Add to playlist” in any song's menu." else "Nothing here yet") }
    }
    if (renaming) {
        NameDialog("Rename playlist", p.name, "Rename") { name ->
            if (name != null && name != p.name) app.scope.launch { app.store.renamePlaylist(p, name) }
            renaming = false
        }
    }
    if (deleting) {
        AlertDialog(
            { deleting = false },
            confirmButton = {
                TextButton({
                    deleting = false
                    app.scope.launch { app.store.deletePlaylist(p) }
                    onBack()
                }) { Text("Delete") }
            },
            dismissButton = { TextButton({ deleting = false }) { Text("Keep") } },
            title = { Text("Delete “${p.name}”?") },
            text = { Text("The songs stay in the library. The server keeps a copy of the playlist file in its data folder.") },
        )
    }
}
