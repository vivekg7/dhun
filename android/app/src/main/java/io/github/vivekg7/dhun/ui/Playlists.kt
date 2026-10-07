package io.github.vivekg7.dhun.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
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
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Catalog
import io.github.vivekg7.dhun.data.Mark
import io.github.vivekg7.dhun.data.PlayStat
import io.github.vivekg7.dhun.data.Resume
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.songIds

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
            item { SectionLabel("Automatic") }
            items(ListKind.entries.drop(2), key = { it.name }) { k ->
                NameRow(k.icon, k.label, "${lists(k).size}") { nav.open(Tab.Playlists, Page.ListPage(k)) }
            }
            item { SectionLabel("Playlists") }
        }
        val shown = playlists.filter { q.isEmpty() || Catalog.fold(it.name).contains(q) }
        items(shown, key = { it.id }) { p ->
            val count = songIds(p.songs).count { it != 0L && catalog.byId.containsKey(it) }
            NameRow(Icons.Playlist, p.name, if (p.shared) "$count · shared" else "$count") { nav.open(Tab.Playlists, Page.PlaylistPage(p.id)) }
        }
        if (playlists.isEmpty()) item { Empty("No playlists yet") }
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
        modifier.background(if (filled) c.primary else c.surfaceContainer, RoundedCornerShape(16.dp)).clickable(onClick = onClick).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Icon(kind.icon, null, Modifier.size(26.dp), tint = if (filled) fg else c.secondary)
        Column {
            Text(kind.label, style = MaterialTheme.typography.titleMedium.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Bold), color = fg)
            Text(summary(songs), style = MaterialTheme.typography.bodySmall, color = if (filled) fg.copy(alpha = 0.85f) else c.onSurfaceVariant)
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
        PageHeader(kind.label, summary(songs), onBack, { playList(kind.label, kind.source, songs, 0) }) { shuffleList(kind.label, kind.source, songs) }
    }
}

@Composable
fun PlaylistScreen(
    id: Long,
    nav: Nav,
    onBack: () -> Unit,
) {
    val app = App.app
    val playlists by app.store.playlists.collectAsState(emptyList())
    val catalog by app.catalog.collectAsState()
    val p = playlists.firstOrNull { it.id == id } ?: return Empty("")
    // 0 is an entry the server could not match to a song; it is kept in the file but not playable.
    val songs = catalog.songsOf(songIds(p.songs).filter { it != 0L })
    val source = "playlist:${p.id}"
    SongList(p.name, source, songs, nav) {
        PageHeader(p.name, summary(songs) + if (p.shared) " · shared" else "", onBack, { playList(p.name, source, songs, 0) }) {
            shuffleList(p.name, source, songs)
        }
    }
}
