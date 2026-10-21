package io.github.vivekg7.dhun.ui

import android.content.Intent
import android.speech.RecognizerIntent
import android.util.Log
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
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
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
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
import io.github.vivekg7.dhun.data.Shelf
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.songIds
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
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

    // Under their own headings (docs/plans/031_podcasts_and_audiobooks.md),
    // before the songs, which can run to hundreds and would bury them.
    Books("Audiobooks"),
    Shows("Podcasts"),
    Episodes("Episodes"),
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
    ): Found? {
        val t0 = System.nanoTime()
        val hits = index.search(q, boost)
        // The keystroke's cost, for the speed targets (plan 036): the budget is 50 ms with the drawing.
        Log.d(App.TAG, "search \"${q.whole}\" in ${kind.name.lowercase()}: ${hits.size} of ${index.items.size} in ${(System.nanoTime() - t0) / 1_000_000} ms")
        return hits.takeIf { it.isNotEmpty() }?.let { Found(kind, it.map { h -> h.first as Any }, it.first().second) }
    }
    return listOfNotNull(
        section(Kind.Artists, catalog.artistIndex),
        section(Kind.Albums, catalog.albumIndex),
        section(Kind.Genres, catalog.genreIndex),
        section(Kind.Folders, catalog.folderIndex),
        section(Kind.Playlists, SearchIndex.names(playlists, "playlist") { it.name }),
        section(Kind.Books, catalog.bookIndex),
        section(Kind.Shows, catalog.showIndex),
        section(Kind.Episodes, catalog.episodeIndex),
        section(Kind.Songs, catalog.songIndex, boost),
    ).sortedBy { it.best }
}

// The last results and what they were for, kept while the tab is away.
private var lastFound: Pair<List<Any>, List<Found>> = emptyList<Any>() to emptyList()
private var lastInLyrics: Pair<List<Any>, List<Pair<Song, String>>?> = emptyList<Any>() to emptyList()

@Composable
fun SearchScreen(nav: Nav) {
    val app = App.app
    val catalog by app.catalog.collectAsState()
    val playlists by app.store.playlists.collectAsState()
    val favorites by app.store.favorites.collectAsState()
    val stats by app.store.playStats.collectAsState()
    var query by rememberSaveable { mutableStateOf("") }
    val q = remember(query) { SearchQuery(query) }
    // Off the main thread: the first search builds the catalog's indexes.
    // Starts from the last results when they still hold: the tab coming back
    // from one must show them in its first frame, or its saved scroll is lost.
    val foundFor = listOf(catalog, playlists, query)
    val found by produceState(lastFound.takeIf { it.first == foundFor }?.second ?: emptyList(), catalog, playlists, q) {
        val favs = favorites.map { it.song }.toSet()
        val plays = stats.associate { it.song to it.count }
        value =
            withContext(Dispatchers.Default) {
                find(catalog, playlists, q) { s -> (if (s.id in favs) 3 else 0) + minOf(2, (plays[s.id] ?: 0) / 5) }
            }
        lastFound = foundFor to value
    }
    // The server's, once typing stops; null while it is asked.
    val lyricsFor = listOf(catalog, query)
    val inLyrics by produceState<List<Pair<Song, String>>?>(lastInLyrics.takeIf { it.first == lyricsFor }?.second ?: emptyList(), catalog, q) {
        if (q.lyrics.length < 3) {
            value = emptyList()
            return@produceState
        }
        if (lastInLyrics.first == lyricsFor) return@produceState
        value = null
        delay(400)
        value = app.lyrics.search(q).mapNotNull { h: LyricsHit -> catalog.byId[h.song]?.let { it to h.line } }
        lastInLyrics = lyricsFor to value
    }
    var expanded by rememberSaveable(query) { mutableStateOf("") }
    val heard = rememberHeard()
    val keep = { app.prefs.keepSearch(query) }
    val list = rememberLazyListState()
    val scope = rememberCoroutineScope()
    // The Search shortcut on the icon: the box takes focus, and the flag goes once it has.
    val focusAsked by app.intents.focusSearch.collectAsState()
    if (focusAsked) LaunchedEffect(Unit) { app.intents.focusSearch.value = false }
    // The system's recogniser, where one is installed; what it heard goes into the box as if typed (docs/plans/038_voice_search_and_assistant.md).
    val context = LocalContext.current
    val canHear = remember { listen().resolveActivity(context.packageManager) != null }
    val hear =
        rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
            r.data
                ?.getStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS)
                ?.firstOrNull()
                ?.let { query = it }
        }

    fun whole(f: Found) = f.kind == Kind.Songs || f.kind.name in expanded.split(',')

    fun shown(f: Found) =
        when {
            f.kind == Kind.Songs -> f.hits.take(SONGS_SHOWN)
            whole(f) -> f.hits
            else -> f.hits.take(SHOWN)
        }

    fun more(f: Found) = !whole(f) && f.hits.size > SHOWN

    val lyrics = inLyrics
    // Each section's chip and where its heading is in the list: after the
    // field, and the chips themselves when there are two sections to jump between.
    val names = found.map { it.kind.label } + if (lyrics.isNullOrEmpty()) emptyList() else listOf("Lyrics")
    val chips = query.isNotBlank() && names.size > 1
    val starts =
        found
            .runningFold(
                if (chips) 2 else 1,
            ) { at, f -> at + 1 + shown(f).size + if (more(f)) 1 else 0 }
            .let { if (lyrics.isNullOrEmpty()) it.dropLast(1) else it }
    var chipsHeight by remember { mutableIntStateOf(0) }
    val atTop = starts.indexOfLast { it <= list.firstVisibleItemIndex + 1 }.coerceAtLeast(0)

    FastScroll(list, Modifier.fillMaxSize()) {
        LazyColumn(Modifier.fillMaxSize(), list) {
            item {
                SearchField(query, { query = it }, "Search songs, albums, artists, lyrics", autofocus = focusAsked) {
                    if (canHear) Tip("Search by voice") { IconButton({ hear.launch(listen()) }) { Icon(Icons.Mic, "Search by voice", Modifier.size(22.dp)) } }
                }
            }
            if (query.isBlank()) {
                recent(app.prefs.recentSearches, { query = it }, catalog.songs.size)
                return@LazyColumn
            }
            if (chips) {
                stickyHeader(key = "chips") {
                    SectionChips(names, atTop, { chipsHeight = it }) { i -> scope.launch { list.animateScrollToItem(starts[i], -chipsHeight) } }
                }
            }
            for (f in found) {
                item(key = "label/${f.kind}") { SectionLabel(f.kind.label) }
                val shown = shown(f)
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
                    items(shown, key = { "${f.kind}/${keyOf(it)}" }) { h -> FoundRow(h, f.kind, catalog, heard, nav, keep) }
                }
                if (more(f)) {
                    item(key = "more/${f.kind}") {
                        NameRow(Icons.ExpandMore, "Show all ${f.hits.size} ${f.kind.label.lowercase()}", "") { expanded += ",${f.kind.name}" }
                    }
                }
            }
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
}

/**
 * What the mic asks of the recogniser: free speech, on the device where it
 * can (Android 12 phones without the model use the online one), in the
 * phone's language. Devanagari folds to Latin in search, so Hindi needs
 * nothing more.
 */
private fun listen(): Intent =
    Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
        .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
        .putExtra(RecognizerIntent.EXTRA_PREFER_OFFLINE, true)
        .putExtra(RecognizerIntent.EXTRA_PROMPT, "Say a song, an artist, an album or a line")

/**
 * The sections found, one tap away (docs/plans/029_search.md). Pinned under
 * the field while the results scroll; the section at the top is the one
 * selected, and the row scrolls to keep it in sight.
 */
@Composable
private fun SectionChips(
    names: List<String>,
    selected: Int,
    onHeight: (Int) -> Unit,
    onPick: (Int) -> Unit,
) {
    val row = rememberLazyListState()
    LaunchedEffect(selected) {
        val seen = row.layoutInfo.visibleItemsInfo.find { it.index == selected }
        if (seen == null || seen.offset < 0 || seen.offset + seen.size > row.layoutInfo.viewportEndOffset) row.animateScrollToItem(selected)
    }
    LazyRow(
        Modifier
            .fillMaxWidth()
            .background(MaterialTheme.colorScheme.background)
            .onSizeChanged { onHeight(it.height) },
        row,
        PaddingValues(horizontal = 16.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        itemsIndexed(names) { i, name -> FilterChip(i == selected, { onPick(i) }, { Text(name) }) }
    }
}

private fun keyOf(h: Any): Any =
    when (h) {
        is Group -> h.name
        is Album -> h.key
        is Folder -> h.path
        is Playlist -> h.id
        is Shelf -> h.key
        is Song -> h.id
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
    heard: Heard,
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

        is Shelf -> {
            ShelfRow(h, if (h.isBook) listOf(h.author, heard.progress(h)).filter { it.isNotEmpty() }.joinToString(" · ") else "Podcast") {
                open(Page.ShelfPage(h.key))
            }
        }

        // An episode found plays its show from there, as on the show's page.
        is Song -> {
            val shelf = catalog.shelfOf(h)
            SongRow(h, onClick = {
                keep()
                if (shelf != null) playShelf(nav, shelf, heard, h)
            }, menu = SongMenu(nav = nav), detail = listOf(h.groupTitle, h.day).filter { it.isNotEmpty() }.joinToString(" · "))
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
