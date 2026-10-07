package io.github.vivekg7.dhun.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.SaveableStateHolder
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** Musicolet's tabs, in its order (docs/plans/011_android_app.md). */
enum class Tab(
    val icon: ImageVector,
    val label: String,
) {
    Queues(Icons.Queues, "Queues"),
    Now(Icons.NowPlaying, "Now playing"),
    Folders(Icons.Folder, "Folders"),
    Albums(Icons.Album, "Albums"),
    Artists(Icons.Artist, "Artists"),
    Genres(Icons.Genre, "Genres"),
    Playlists(Icons.Playlists, "Playlists"),
    Search(Icons.Search, "Search"),
}

/** A page opened inside a tab, on top of the tab's own list. */
sealed interface Page {
    data class AlbumPage(
        val key: String,
    ) : Page

    data class ArtistPage(
        val name: String,
    ) : Page

    data class GenrePage(
        val name: String,
    ) : Page

    data class FolderPage(
        val path: String,
    ) : Page

    data class PlaylistPage(
        val id: Long,
    ) : Page

    data object DownloadsPage : Page

    /** Favorites, Listen Later, or one of the automatic views ([ListKind]). */
    data class ListPage(
        val kind: ListKind,
    ) : Page
}

/** A page as one string, for saving: its kind, a colon, what it shows. */
private fun Page.encode(): String =
    when (this) {
        is Page.AlbumPage -> "album:$key"
        is Page.ArtistPage -> "artist:$name"
        is Page.GenrePage -> "genre:$name"
        is Page.FolderPage -> "folder:$path"
        is Page.PlaylistPage -> "playlist:$id"
        Page.DownloadsPage -> "downloads:"
        is Page.ListPage -> "list:${kind.name}"
    }

private fun decodePage(s: String): Page? {
    val kind = s.substringBefore(':')
    val arg = s.substringAfter(':')
    return when (kind) {
        "album" -> Page.AlbumPage(arg)
        "artist" -> Page.ArtistPage(arg)
        "genre" -> Page.GenrePage(arg)
        "folder" -> Page.FolderPage(arg)
        "playlist" -> arg.toLongOrNull()?.let(Page::PlaylistPage)
        "downloads" -> Page.DownloadsPage
        "list" -> ListKind.entries.firstOrNull { it.name == arg }?.let(Page::ListPage)
        else -> null
    }
}

/**
 * Each tab keeps its own stack of pages, so going to an album and back leaves the other tabs as they were.
 *
 * Whatever is under the top page leaves composition, and with it the state its
 * `rememberSaveable` keeps (a search, a scroll). [saved] holds that state per
 * page until the page is popped, so going back finds it as it was.
 */
class Nav(
    private val saved: SaveableStateHolder,
) {
    private val stacks = Tab.entries.associateWith { mutableStateListOf<Page>() }
    var tab by mutableStateOf(Tab.Now)

    /** The settings screen shown over the tabs, or null. */
    var settings by mutableStateOf<SettingsPage?>(null)

    /** Back from a category goes to the list of them, and from there to the tabs. */
    fun settingsBack() {
        settings = if (settings == SettingsPage.Main) null else SettingsPage.Main
    }

    fun top(t: Tab) = stacks.getValue(t).lastOrNull()

    fun canPop(t: Tab) = stacks.getValue(t).isNotEmpty()

    /**
     * The key of [t]'s top page, or of its own list, in [saved]. The page is
     * in it so that one page's scroll can never reach another at the same
     * depth, whatever happens to the stacks.
     */
    fun key(t: Tab) = "${t.name}/${stacks.getValue(t).size}/${top(t)}"

    fun pop(t: Tab) {
        if (!canPop(t)) return
        // A page popped is gone: opening one again starts it afresh.
        saved.removeState(key(t))
        stacks.getValue(t).removeAt(stacks.getValue(t).lastIndex)
    }

    fun open(
        t: Tab,
        page: Page,
    ) {
        stacks.getValue(t).add(page)
        tab = t
    }

    companion object {
        /**
         * Android recreates the activity on a rotation, a dark-mode or font
         * change, a resize, or after reclaiming the app in the background:
         * the open pages and the tab come back with it.
         */
        fun saver(saved: SaveableStateHolder) =
            listSaver<Nav, Any>(
                save = { n -> listOf(n.tab.name, n.settings?.name ?: "") + Tab.entries.map { t -> ArrayList(n.stacks.getValue(t).map { it.encode() }) } },
                restore = { l ->
                    Nav(saved).apply {
                        tab = Tab.entries.firstOrNull { it.name == l[0] } ?: Tab.Now
                        settings = SettingsPage.entries.firstOrNull { it.name == l[1] }
                        Tab.entries.forEachIndexed { i, t -> (l[2 + i] as List<*>).mapNotNullTo(stacks.getValue(t)) { decodePage(it as String) } }
                    }
                },
            )
    }
}

@Composable
fun Shell() {
    val saved = rememberSaveableStateHolder()
    val nav = rememberSaveable(saver = Nav.saver(saved)) { Nav(saved) }
    val pager = rememberPagerState(initialPage = Tab.Now.ordinal) { Tab.entries.size }
    val scope = rememberCoroutineScope()
    // The pager and nav.tab drive each other: a swipe sets the tab, "Go to album" moves the pager.
    LaunchedEffect(pager.currentPage) { nav.tab = Tab.entries[pager.currentPage] }
    LaunchedEffect(nav.tab) { if (pager.currentPage != nav.tab.ordinal) pager.animateScrollToPage(nav.tab.ordinal) }

    BackHandler(nav.settings != null) { nav.settingsBack() }
    BackHandler(nav.settings == null && nav.canPop(nav.tab)) { nav.pop(nav.tab) }

    // Settings replaces the tabs: they keep their state for coming back.
    val settings = nav.settings
    if (settings != null) {
        Box(
            Modifier
                .fillMaxSize()
                .background(
                    MaterialTheme.colorScheme.background,
                ).windowInsetsPadding(WindowInsets.statusBars)
                .windowInsetsPadding(WindowInsets.navigationBars),
        ) {
            SettingsScreen(settings, { nav.settings = it }, nav::settingsBack)
        }
        return
    }
    saved.SaveableStateProvider("tabs") {
        Column(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
            HorizontalPager(pager, Modifier.weight(1f).windowInsetsPadding(WindowInsets.statusBars), key = { it }) { page ->
                val tab = Tab.entries[page]
                Box(Modifier.fillMaxSize()) {
                    saved.SaveableStateProvider(nav.key(tab)) {
                        when (val top = nav.top(tab)) {
                            null -> TabRoot(tab, nav)
                            else -> PageContent(tab, top, nav)
                        }
                    }
                }
            }
            HandoffBar()
            if (nav.tab != Tab.Now) MiniPlayer { nav.tab = Tab.Now }
            TabBar(nav.tab, onSelect = { t ->
                // Tapping the tab you are on goes back to its own list.
                if (t == nav.tab) while (nav.canPop(t)) nav.pop(t)
                nav.tab = t
                scope.launch { pager.scrollToPage(t.ordinal) }
            }, onSettings = { nav.settings = SettingsPage.Main })
        }
    }
}

@Composable
private fun TabRoot(
    tab: Tab,
    nav: Nav,
) = when (tab) {
    Tab.Queues -> QueuesScreen(nav)
    Tab.Now -> NowPlayingScreen(nav)
    Tab.Folders -> FolderScreen("", nav, root = true)
    Tab.Albums -> AlbumsScreen(nav)
    Tab.Artists -> GroupsScreen(nav, artists = true)
    Tab.Genres -> GroupsScreen(nav, artists = false)
    Tab.Playlists -> PlaylistsScreen(nav)
    Tab.Search -> SearchScreen(nav)
}

@Composable
private fun PageContent(
    tab: Tab,
    page: Page,
    nav: Nav,
) {
    val back = {
        nav.pop(tab)
        Unit
    }
    when (page) {
        is Page.AlbumPage -> AlbumScreen(page.key, nav, back)
        is Page.ArtistPage -> GroupScreen(page.name, artists = true, nav, back)
        is Page.GenrePage -> GroupScreen(page.name, artists = false, nav, back)
        is Page.FolderPage -> FolderScreen(page.path, nav, root = false, onBack = back)
        is Page.PlaylistPage -> PlaylistScreen(page.id, nav, back)
        is Page.ListPage -> ListScreen(page.kind, nav, back)
        Page.DownloadsPage -> DownloadsScreen(nav, back)
    }
}

@Composable
private fun TabBar(
    current: Tab,
    onSelect: (Tab) -> Unit,
    onSettings: () -> Unit,
) {
    val c = MaterialTheme.colorScheme
    val app = App.app
    Column(Modifier.background(c.surfaceContainer).windowInsetsPadding(WindowInsets.navigationBars)) {
        HorizontalDivider(color = c.outlineVariant)
        Row(Modifier.fillMaxWidth().height(60.dp)) {
            for (t in Tab.entries) {
                val selected = t == current
                Box(Modifier.weight(1f).fillMaxHeight()) {
                    Tip(t.label) {
                        Box(Modifier.fillMaxSize().clickable { onSelect(t) }, contentAlignment = Alignment.Center) {
                            if (selected) {
                                Box(
                                    Modifier
                                        .align(
                                            Alignment.TopCenter,
                                        ).width(32.dp)
                                        .height(3.dp)
                                        .background(c.primary, RoundedCornerShape(bottomStart = 2.dp, bottomEnd = 2.dp)),
                                )
                            }
                            Icon(t.icon, t.label, Modifier.size(24.dp), tint = if (selected) c.primary else c.onSurfaceVariant.copy(alpha = 0.8f))
                        }
                    }
                }
            }
            var menu by remember { mutableStateOf(false) }
            var sleep by remember { mutableStateOf(false) }
            if (sleep) SleepDialog { sleep = false }
            Box(Modifier.weight(1f).fillMaxHeight(), contentAlignment = Alignment.Center) {
                Tip("Menu") { IconButton({ menu = true }) { Icon(Icons.More, "Menu", tint = c.onSurfaceVariant) } }
                DropdownMenu(menu, { menu = false }) {
                    val close = { menu = false }
                    MenuItem(sleepState()?.let { "Sleep timer · $it" } ?: "Sleep timer", close) { sleep = true }
                    MenuItem("Sync now", close) { app.scope.launch { app.sync.now() } }
                    MenuItem("Settings", close, onSettings)
                }
            }
        }
    }
}

/**
 * The owner's addition to Musicolet's layout: a one-line player on every
 * tab but Now playing, so pausing doesn't need a tab switch.
 */
@Composable
private fun MiniPlayer(onOpen: () -> Unit) {
    val app = App.app
    val song by app.playback.current.collectAsState()
    val playing by app.playback.playing.collectAsState()
    val s = song ?: return
    val c = MaterialTheme.colorScheme
    var progress by remember { mutableFloatStateOf(0f) }
    LaunchedEffect(s.id, playing) {
        while (true) {
            val d = app.playback.player.duration
            progress =
                if (d > 0) {
                    app.playback.player.currentPosition
                        .toFloat() / d
                } else {
                    0f
                }
            if (!playing) break
            delay(500)
        }
    }
    Column(Modifier.background(c.surfaceContainerHigh).clickable(onClick = onOpen)) {
        Box(Modifier.fillMaxWidth().height(2.dp).background(c.outline)) {
            Box(Modifier.fillMaxWidth(progress.coerceIn(0f, 1f)).height(2.dp).background(c.primary))
        }
        Row(Modifier.height(56.dp).padding(start = 12.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Art(s, 40.dp, RoundedCornerShape(6.dp))
            Column(Modifier.weight(1f).padding(start = 12.dp)) {
                Text(s.title, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(s.displayArtist, style = MaterialTheme.typography.bodySmall, color = c.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Tip(if (playing) "Pause" else "Play") {
                IconButton({ if (playing) app.playback.player.pause() else app.playback.player.play() }) {
                    Icon(if (playing) Icons.Pause else Icons.Play, if (playing) "Pause" else "Play", Modifier.size(28.dp))
                }
            }
            Tip("Next") { IconButton({ app.playback.player.seekToNext() }) { Icon(Icons.Next, "Next", Modifier.size(26.dp)) } }
        }
    }
}

/**
 * "Continue from MacBook" (docs/plans/017_handoff.md), above the mini player
 * on every tab, so it is seen wherever the app opens without blocking it.
 */
@Composable
private fun HandoffBar() {
    val app = App.app
    val offer by app.playback.handoff.collectAsState()
    val np = offer ?: return
    val song = app.catalog.value.byId[np.song] ?: return
    val c = MaterialTheme.colorScheme
    Row(
        Modifier.fillMaxWidth().background(c.primaryContainer).padding(start = 16.dp, end = 4.dp, top = 4.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Devices, null, Modifier.size(22.dp), tint = c.onPrimaryContainer)
        Column(Modifier.weight(1f).padding(start = 12.dp)) {
            Text(
                "Continue from ${np.deviceName.ifEmpty { "another device" }}",
                style = MaterialTheme.typography.labelLarge,
                color = c.onPrimaryContainer,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                "${song.title} · ${duration(app.playback.placeOf(np))}",
                style = MaterialTheme.typography.bodySmall,
                color = c.onPrimaryContainer,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        TextButton({ app.playback.continueFrom(np) }) { Text("Continue") }
        Tip("Dismiss") { IconButton({ app.playback.dismissHandoff(np) }) { Icon(Icons.Close, "Dismiss", tint = c.onPrimaryContainer) } }
    }
}
