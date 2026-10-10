package io.github.vivekg7.dhun.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Album
import io.github.vivekg7.dhun.data.Catalog
import io.github.vivekg7.dhun.data.Downloads
import io.github.vivekg7.dhun.data.Folder
import io.github.vivekg7.dhun.data.Group
import io.github.vivekg7.dhun.data.LyricsHit
import io.github.vivekg7.dhun.data.Playlist
import io.github.vivekg7.dhun.data.SearchIndex
import io.github.vivekg7.dhun.data.SearchQuery
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.songIds
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

/** The Search tab's sections (docs/plans/029_search.md). */
private enum class Kind(
    val label: String,
) {
    Artists("Artists"),
    Albums("Albums"),
    Genres("Genres"),
    Folders("Folders"),
    Playlists("Playlists"),
    Songs("Songs"),
}

private class Found(
    val kind: Kind,
    val hits: List<Any>,
    val best: Int,
)

/** Results a section shows before "Show all": one strong song should not sit under twenty albums. */
private const val SHOWN = 3

/** Songs found are many in a big library; past this many, the query is too loose to read through. */
private const val SONGS_SHOWN = 300

/** Every section with something in it, the one with the best match first; on a tie, in [Kind]'s order. */
private fun find(
    catalog: Catalog,
    playlists: List<Playlist>,
    q: SearchQuery,
    boost: (Song) -> Int,
): List<Found> {
    if (q.isEmpty) return emptyList()

    fun <T> section(
        kind: Kind,
        index: SearchIndex<T>,
        boost: (T) -> Int = { 0 },
    ) = index.search(q, boost).takeIf { it.isNotEmpty() }?.let { hits -> Found(kind, hits.map { it.first as Any }, hits.first().second) }
    return listOfNotNull(
        section(Kind.Artists, catalog.artistIndex),
        section(Kind.Albums, catalog.albumIndex),
        section(Kind.Genres, catalog.genreIndex),
        section(Kind.Folders, catalog.folderIndex),
        section(Kind.Playlists, SearchIndex.names(playlists, "playlist") { it.name }),
        section(Kind.Songs, catalog.songIndex, boost),
    ).sortedBy { it.best }
}

@Composable
fun SearchScreen(nav: Nav) {
    val app = App.app
    val catalog by app.catalog.collectAsState()
    val playlists by app.store.playlists.collectAsState(emptyList())
    val favorites by app.store.favorites.collectAsState(emptyList())
    val stats by app.store.playStats.collectAsState(emptyList())
    var query by rememberSaveable { mutableStateOf("") }
    val q = remember(query) { SearchQuery(query) }
    // Off the main thread: the first search builds the catalog's indexes.
    val found by produceState(emptyList(), catalog, playlists, q) {
        val favs = favorites.map { it.song }.toSet()
        val plays = stats.associate { it.song to it.count }
        value =
            withContext(Dispatchers.Default) {
                find(catalog, playlists, q) { s -> (if (s.id in favs) 3 else 0) + minOf(2, (plays[s.id] ?: 0) / 5) }
            }
    }
    // The server's, once typing stops; null while it is asked.
    val inLyrics by produceState<List<Pair<Song, String>>?>(emptyList(), catalog, q) {
        if (q.lyrics.length < 3) {
            value = emptyList()
            return@produceState
        }
        value = null
        delay(400)
        value = app.lyrics.search(q).mapNotNull { h: LyricsHit -> catalog.byId[h.song]?.let { it to h.line } }
    }
    var expanded by rememberSaveable(query) { mutableStateOf("") }
    val keep = { app.prefs.keepSearch(query) }

    LazyColumn(Modifier.fillMaxSize()) {
        item { SearchField(query, { query = it }, "Search songs, albums, artists, lyrics") }
        if (query.isBlank()) {
            recent(app.prefs.recentSearches, { query = it }, catalog.songs.size)
            return@LazyColumn
        }
        for (f in found) {
            item(key = "label/${f.kind}") { SectionLabel(f.kind.label) }
            val whole = f.kind == Kind.Songs || f.kind.name in expanded.split(',')
            val shown =
                if (f.kind == Kind.Songs) {
                    f.hits.take(SONGS_SHOWN)
                } else if (whole) {
                    f.hits
                } else {
                    f.hits.take(SHOWN)
                }
            if (f.kind == Kind.Songs) {
                @Suppress("UNCHECKED_CAST")
                val songs = shown as List<Song>
                itemsIndexed(songs, key = { _, s -> "song/${s.id}" }) { i, s ->
                    SongRow(s, onClick = {
                        keep()
                        playList(nav, "Search: ${query.trim()}", "search", songs, i)
                    }, menu = SongMenu(nav = nav))
                }
            } else {
                items(shown, key = { "${f.kind}/${keyOf(it)}" }) { h -> FoundRow(h, f.kind, catalog, nav, keep) }
            }
            if (!whole && f.hits.size > SHOWN) {
                item(key = "more/${f.kind}") {
                    NameRow(Icons.ExpandMore, "Show all ${f.hits.size} ${f.kind.label.lowercase()}", "") { expanded += ",${f.kind.name}" }
                }
            }
        }
        val lyrics = inLyrics
        if (lyrics == null || lyrics.isNotEmpty()) item(key = "label/lyrics") { SectionLabel("In lyrics") }
        if (lyrics == null) {
            item(key = "lyrics/wait") { Empty("Searching lyrics…") }
        } else {
            val songs = lyrics.map { it.first }
            itemsIndexed(lyrics, key = { _, (s, _) -> "lyrics/${s.id}" }) { i, (s, line) ->
                SongRow(s, onClick = {
                    keep()
                    playList(nav, "Lyrics: ${query.trim()}", "search", songs, i)
                }, menu = SongMenu(nav = nav), detail = "“$line”")
            }
        }
        if (found.isEmpty() && lyrics?.isEmpty() == true) item { Empty("Nothing matches “${query.trim()}”") }
    }
}

private fun keyOf(h: Any): Any =
    when (h) {
        is Group -> h.name
        is Album -> h.key
        is Folder -> h.path
        is Playlist -> h.id
        else -> h.hashCode()
    }

/** The last searches, each a tap away; or, with none yet, what the box searches. */
private fun androidx.compose.foundation.lazy.LazyListScope.recent(
    searches: List<String>,
    onPick: (String) -> Unit,
    songs: Int,
) {
    if (searches.isEmpty()) {
        if (songs > 0) {
            item {
                Empty(
                    "Search $songs songs by title, artist, album, composer, genre or lyrics.\n" +
                        "Narrow a word with artist:, album:, title:, genre:, year: or lyrics:",
                )
            }
        }
        return
    }
    item {
        Row(verticalAlignment = Alignment.CenterVertically) {
            SectionLabel("Recent searches", Modifier.weight(1f))
            TextButton({ App.app.prefs.forgetSearch(null) }, Modifier.padding(end = 8.dp)) { Text("Clear") }
        }
    }
    items(searches, key = { "recent/$it" }) { s ->
        Row(
            Modifier
                .fillMaxWidth()
                .clickable { onPick(s) }
                .height(56.dp)
                .padding(start = 20.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(Icons.History, null, Modifier.size(22.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.width(16.dp))
            Text(s, Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Tip("Forget") { IconButton({ App.app.prefs.forgetSearch(s) }) { Icon(Icons.Close, "Forget", Modifier.size(20.dp)) } }
        }
    }
}

/** An artist, album, genre, folder or playlist found; it opens in the Search tab, so Back returns to the results. */
@Composable
private fun FoundRow(
    h: Any,
    kind: Kind,
    catalog: Catalog,
    nav: Nav,
    keep: () -> Unit,
) {
    fun open(page: Page) {
        keep()
        nav.open(Tab.Search, page)
    }
    when (h) {
        is Group -> {
            if (kind == Kind.Artists) {
                NameRow(Icons.Artist, h.name, "${h.songs.size}", pinned(Downloads.ARTIST, h.name)) { open(Page.ArtistPage(h.name)) }
            } else {
                NameRow(Icons.Genre, h.name, "${h.songs.size}", pinned(Downloads.GENRE, h.name)) { open(Page.GenrePage(h.name)) }
            }
        }

        is Album -> {
            TwoLines(h.name, listOfNotNull(h.artist, h.year.takeIf { it > 0 }?.toString()).joinToString(" · "), { open(Page.AlbumPage(h.key)) }) {
                Art(h.songs.first(), SmallArt, RoundedCornerShape(6.dp))
            }
        }

        // Many folders share a name ("CD1"), so the path says which.
        is Folder -> {
            TwoLines(h.name, h.path.substringBeforeLast('/', "Music").replace("/", " › "), { open(Page.FolderPage(h.path)) }) {
                Box(Modifier.size(SmallArt), contentAlignment = Alignment.Center) {
                    Icon(Icons.Folder, null, Modifier.size(22.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }

        is Playlist -> {
            val count = songIds(h.songs).count { it != 0L && catalog.byId.containsKey(it) }
            NameRow(Icons.Playlist, h.name, "$count", pinned(Downloads.PLAYLIST, h.id.toString())) { open(Page.PlaylistPage(h.id)) }
        }
    }
}

@Composable
private fun TwoLines(
    title: String,
    detail: String,
    onClick: () -> Unit,
    leading: @Composable () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .height(64.dp)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        leading()
        Column(Modifier.weight(1f).padding(start = 14.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                detail,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}
