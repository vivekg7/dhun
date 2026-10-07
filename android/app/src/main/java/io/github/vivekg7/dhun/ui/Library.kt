package io.github.vivekg7.dhun.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
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
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Catalog
import io.github.vivekg7.dhun.data.Downloads
import io.github.vivekg7.dhun.data.Song

/** Plays [songs] from [index] in a new queue named after the list (AGENTS.md). */
fun playList(
    name: String,
    source: String,
    songs: List<Song>,
    index: Int,
) = App.app.playback.play(name, source, songs, index)

fun shuffleList(
    name: String,
    source: String,
    songs: List<Song>,
) {
    if (songs.isNotEmpty()) App.app.playback.play(name, source, songs.shuffled(), 0)
}

/** A list of songs where tapping one plays the whole list from there. */
@Composable
fun SongList(
    name: String,
    source: String,
    songs: List<Song>,
    nav: Nav,
    header: @Composable () -> Unit = {},
) {
    LazyColumn(Modifier.fillMaxSize()) {
        item { header() }
        itemsIndexed(songs, key = { i, s -> "$i/${s.id}" }) { i, s ->
            SongRow(s, onClick = { playList(name, source, songs, i) }, menu = SongMenu(nav = nav))
        }
        if (songs.isEmpty()) item { Empty("Nothing here yet") }
    }
}

/** Whether this album, folder, artist or genre is downloaded, for its mark in a list. */
@Composable
fun pinned(
    kind: String,
    ref: String,
): Boolean {
    val pins by App.app.downloads.pins
        .collectAsState()
    val key = Downloads.key(kind, ref)
    return pins.any { it.key == key }
}

@Composable
fun Empty(text: String) =
    Box(Modifier.fillMaxWidth().padding(48.dp), contentAlignment = Alignment.Center) {
        Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center)
    }

/** The real folder tree of the NAS, as Musicolet's hierarchical Folders tab. */
@Composable
fun FolderScreen(
    path: String,
    nav: Nav,
    root: Boolean,
    onBack: () -> Unit = {},
) {
    val catalog by App.app.catalog.collectAsState()
    val folder = catalog.folders[path]
    if (folder == null) {
        Empty(if (catalog.songs.isEmpty()) "The library is loading…" else "This folder is gone")
        return
    }
    LazyColumn(Modifier.fillMaxSize()) {
        item {
            if (root) {
                SectionLabel("Music · ${catalog.songs.size} songs")
            } else {
                PageHeader(
                    folder.name,
                    path.replace("/", " › "),
                    onBack,
                    { playList(folder.name, "folder:$path", folder.allSongs(), 0) },
                    DownloadTarget(Downloads.FOLDER, path, folder.name, folder.allSongs()),
                ) {
                    shuffleList(folder.name, "folder:$path", folder.allSongs())
                }
            }
        }
        items(folder.children, key = { "f/" + it.path }) { f ->
            NameRow(Icons.Folder, f.name, "${f.allSongs().size}", pinned(Downloads.FOLDER, f.path)) { nav.open(Tab.Folders, Page.FolderPage(f.path)) }
        }
        val songs = folder.songs
        itemsIndexed(songs, key = { _, s -> s.id }) { i, s ->
            SongRow(s, onClick = { playList(folder.name, "folder:$path", songs, i) }, menu = SongMenu(nav = nav))
        }
    }
}

@Composable
fun AlbumsScreen(nav: Nav) {
    val catalog by App.app.catalog.collectAsState()
    var query by rememberSaveable { mutableStateOf("") }
    val albums =
        remember(catalog, query) {
            val q = Catalog.fold(query)
            if (q.isEmpty()) catalog.albums else catalog.albums.filter { Catalog.fold(it.name).contains(q) || Catalog.fold(it.artist).contains(q) }
        }
    Column(Modifier.fillMaxSize()) {
        SearchField(query, { query = it }, "Search ${catalog.albums.size} albums…")
        LazyVerticalGrid(
            GridCells.Adaptive(112.dp),
            Modifier.fillMaxSize(),
            contentPadding = PaddingValues(12.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            items(albums, key = { it.key }) { a ->
                Column(Modifier.clickable { nav.open(Tab.Albums, Page.AlbumPage(a.key)) }) {
                    androidx.compose.foundation.layout.BoxWithConstraints {
                        Art(a.songs.first(), maxWidth, RoundedCornerShape(10.dp))
                    }
                    Row(Modifier.padding(top = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            a.name,
                            Modifier.weight(1f, fill = false),
                            style = MaterialTheme.typography.titleSmall,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        if (pinned(Downloads.ALBUM, a.key)) {
                            Icon(Icons.Downloaded, "Downloaded", Modifier.padding(start = 4.dp).size(14.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                    Text(
                        a.artist,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
}

@Composable
fun AlbumScreen(
    key: String,
    nav: Nav,
    onBack: () -> Unit,
) {
    val catalog by App.app.catalog.collectAsState()
    val album = catalog.albums.firstOrNull { it.key == key } ?: return Empty("This album is gone")
    val facts = listOfNotNull(album.artist, album.year.takeIf { it > 0 }?.toString(), summary(album.songs)).joinToString(" · ")
    SongList(album.name, "album:${album.name}", album.songs, nav) {
        Column {
            PageHeader(
                album.name,
                facts,
                onBack,
                { playList(album.name, "album:${album.name}", album.songs, 0) },
                DownloadTarget(Downloads.ALBUM, album.key, album.name, album.songs),
            ) {
                shuffleList(album.name, "album:${album.name}", album.songs)
            }
        }
    }
}

/** Artists or genres: a searchable list of names. */
@Composable
fun GroupsScreen(
    nav: Nav,
    artists: Boolean,
) {
    val catalog by App.app.catalog.collectAsState()
    var query by rememberSaveable { mutableStateOf("") }
    val all = if (artists) catalog.artists else catalog.genres
    val shown =
        remember(all, query) {
            val q = Catalog.fold(query)
            if (q.isEmpty()) all else all.filter { Catalog.fold(it.name).contains(q) }
        }
    LazyColumn(Modifier.fillMaxSize()) {
        item { SearchField(query, { query = it }, if (artists) "Search ${all.size} artists…" else "Search ${all.size} genres…") }
        items(shown, key = { it.name }) { g ->
            NameRow(if (artists) Icons.Artist else Icons.Genre, g.name, "${g.songs.size}", pinned(if (artists) Downloads.ARTIST else Downloads.GENRE, g.name)) {
                nav.open(if (artists) Tab.Artists else Tab.Genres, if (artists) Page.ArtistPage(g.name) else Page.GenrePage(g.name))
            }
        }
    }
}

@Composable
fun GroupScreen(
    name: String,
    artists: Boolean,
    nav: Nav,
    onBack: () -> Unit,
) {
    val catalog by App.app.catalog.collectAsState()
    val group =
        (if (artists) catalog.artists else catalog.genres).firstOrNull { it.name.equals(name, ignoreCase = true) }
            ?: return Empty("Nothing by $name")
    // An artist's songs by album, then track: how they were released.
    val songs = remember(group) { group.songs.sortedWith(compareBy<Song>({ it.album.lowercase() }, { it.disc }, { it.track })) }
    val source = if (artists) "artist:${group.name}" else "genre:${group.name}"
    SongList(group.name, source, songs, nav) {
        PageHeader(
            group.name,
            summary(songs),
            onBack,
            { playList(group.name, source, songs, 0) },
            DownloadTarget(if (artists) Downloads.ARTIST else Downloads.GENRE, group.name, group.name, songs),
        ) { shuffleList(group.name, source, songs) }
    }
}

@Composable
fun SearchScreen(nav: Nav) {
    val catalog by App.app.catalog.collectAsState()
    var query by rememberSaveable { mutableStateOf("") }
    val hits = remember(catalog, query) { catalog.search(query).take(300) }
    LazyColumn(Modifier.fillMaxSize()) {
        item { SearchField(query, { query = it }, "Search songs, albums, artists") }
        itemsIndexed(hits, key = { _, s -> s.id }) { i, s ->
            SongRow(s, onClick = { playList("Search: ${query.trim()}", "search", hits, i) }, menu = SongMenu(nav = nav))
        }
        if (query.isNotBlank() && hits.isEmpty()) item { Empty("No songs match “${query.trim()}”") }
    }
}
