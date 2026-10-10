package io.github.vivekg7.dhun.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PlainTooltip
import androidx.compose.material3.Text
import androidx.compose.material3.TooltipBox
import androidx.compose.material3.TooltipDefaults
import androidx.compose.material3.rememberTooltipState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Downloads
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.Store
import kotlinx.coroutines.launch

fun duration(ms: Long): String {
    val s = ms / 1000
    return if (s >= 3600) "%d:%02d:%02d".format(s / 3600, s / 60 % 60, s % 60) else "%d:%02d".format(s / 60, s % 60)
}

/** "12 songs · 48 min" */
fun summary(songs: List<Song>): String {
    val min = songs.sumOf { it.durationMs } / 60_000
    val time = if (min >= 60) "${min / 60} h ${min % 60} min" else "$min min"
    return "${songs.size} ${if (songs.size == 1) "song" else "songs"} · $time"
}

@Composable
fun SectionLabel(
    text: String,
    modifier: Modifier = Modifier,
) {
    Text(
        text.uppercase(),
        modifier.padding(start = 20.dp, top = 16.dp, bottom = 4.dp),
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.secondary,
    )
}

/**
 * Musicolet's per-tab quick search box; at the [bottom] of the queue, as there.
 * Only a tap focuses it; the keyboard's Search key, a scroll, a tapped song or
 * leaving the page puts the keyboard away and keeps the query.
 */
@Composable
fun SearchField(
    value: String,
    onChange: (String) -> Unit,
    hint: String,
    bottom: Boolean = false,
    trailing: @Composable () -> Unit = {},
) {
    val focus = LocalFocusManager.current
    Column {
        if (bottom) HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
        Row(Modifier.fillMaxWidth().height(56.dp).padding(start = 20.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Search, null, Modifier.size(22.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.width(12.dp))
            Box(Modifier.weight(1f)) {
                if (value.isEmpty()) {
                    Text(
                        hint,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.bodyLarge,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                BasicTextField(
                    value,
                    onChange,
                    Modifier.fillMaxWidth(),
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                    keyboardActions = KeyboardActions(onSearch = { focus.clearFocus() }),
                    textStyle = MaterialTheme.typography.bodyLarge.copy(color = MaterialTheme.colorScheme.onSurface),
                    cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                )
            }
            if (value.isNotEmpty()) Tip("Clear") { IconButton({ onChange("") }) { Icon(Icons.Close, "Clear", Modifier.size(20.dp)) } }
            trailing()
        }
        if (!bottom) HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
    }
}

/**
 * An icon-only control says what it does when long-pressed, as Android's own
 * do. [tip] is the icon's description for TalkBack too, so the two agree.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun Tip(
    tip: String,
    content: @Composable () -> Unit,
) = TooltipBox(TooltipDefaults.rememberPlainTooltipPositionProvider(), { PlainTooltip { Text(tip) } }, rememberTooltipState(), content = content)

/**
 * A detail page's top: back, title, a line of facts, Play / Shuffle /
 * Download, and a menu: "Add to playlist" for [songs], then [actions].
 */
@Composable
fun PageHeader(
    title: String,
    subtitle: String,
    onBack: () -> Unit,
    onPlay: (() -> Unit)?,
    download: DownloadTarget? = null,
    songs: List<Song> = emptyList(),
    actions: List<Pair<String, () -> Unit>> = emptyList(),
    onShuffle: (() -> Unit)?,
) {
    var adding by remember { mutableStateOf(false) }
    if (adding) AddToPlaylistDialog(songs) { adding = false }
    Column(Modifier.fillMaxWidth().padding(bottom = 8.dp)) {
        Row(Modifier.padding(start = 4.dp, top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Tip("Back") { IconButton(onBack) { Icon(Icons.Back, "Back") } }
            Text(title, Modifier.weight(1f), style = MaterialTheme.typography.headlineSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
            val nas = songs.any { !it.onPhone }
            if (nas || actions.isNotEmpty()) {
                Box {
                    var open by remember { mutableStateOf(false) }
                    Tip("More options") { IconButton({ open = true }) { Icon(Icons.More, "More options") } }
                    DropdownMenu(open, { open = false }) {
                        val close = { open = false }
                        if (nas) MenuItem("Add to playlist…", close) { adding = true }
                        for ((label, action) in actions) MenuItem(label, close, action)
                    }
                }
            }
        }
        Text(subtitle, Modifier.padding(start = 56.dp), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        // An empty list has nothing to play: no buttons that do nothing.
        if (onPlay != null && songs.isNotEmpty()) {
            Row(Modifier.padding(start = 56.dp, top = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilledTonalButton(onPlay) {
                    Icon(Icons.Play, null, Modifier.size(18.dp))
                    Spacer(Modifier.width(6.dp))
                    Text("Play")
                }
                if (onShuffle != null) {
                    FilledTonalButton(onShuffle) {
                        Icon(Icons.Shuffle, null, Modifier.size(18.dp))
                        Spacer(Modifier.width(6.dp))
                        Text("Shuffle")
                    }
                }
                if (download != null && download.songs.any { !it.onPhone }) DownloadButton(download)
            }
        }
    }
}

/** A list row with an icon, a name and a trailing count: folders, artists, genres, views. */
@Composable
fun NameRow(
    icon: ImageVector,
    name: String,
    detail: String,
    downloaded: Boolean = false,
    onClick: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .height(56.dp)
            .padding(horizontal = 20.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, Modifier.size(22.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.width(16.dp))
        Text(name, Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (downloaded) Icon(Icons.Downloaded, "Downloaded", Modifier.padding(end = 8.dp).size(16.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/**
 * One song. The current song of a queue is outlined, as in Musicolet: in
 * the accent in the playing queue, the only colour in a list; in grey and
 * italics in a queue not playing, where it is where that queue will
 * resume. A [selected] row is shaded.
 */
@Composable
fun SongRow(
    song: Song,
    onClick: () -> Unit,
    current: Boolean = false,
    live: Boolean = true,
    selected: Boolean = false,
    onLongClick: (() -> Unit)? = null,
    leading: (@Composable () -> Unit)? = null,
    menu: SongMenu = SongMenu(),
    detail: String = song.displayArtist,
) {
    val c = MaterialTheme.colorScheme
    val app = App.app
    val files by app.downloads.files.collectAsState()
    val reachable by app.api.reachable.collectAsState()
    val cached by app.cache.songs.collectAsState()
    val downloaded = song.id in files
    val playable = song.onPhone || downloaded || song.id in cached
    val outline = if (live) c.primary else c.outline
    val focus = LocalFocusManager.current
    Row(
        Modifier
            .fillMaxWidth()
            .background(if (selected) c.secondaryContainer else c.surface)
            // Drawn inside the row, so its content lines up with the rows around it.
            .then(
                if (current) {
                    Modifier.drawBehind {
                        val inset = Offset(4.dp.toPx(), 2.dp.toPx())
                        drawRoundRect(
                            outline,
                            inset,
                            Size(size.width - 2 * inset.x, size.height - 2 * inset.y),
                            CornerRadius(10.dp.toPx()),
                            style = Stroke(1.dp.toPx()),
                        )
                    }
                } else {
                    Modifier
                },
            ).combinedClickable(onLongClick = onLongClick) {
                focus.clearFocus()
                onClick()
            }.height(64.dp)
            // Offline, what is not on the phone cannot play (docs/plans/012_downloads.md).
            .alpha(if (reachable || playable) 1f else 0.38f)
            .padding(start = if (leading == null) 16.dp else 0.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        leading?.invoke()
        Art(song, SmallArt, RoundedCornerShape(6.dp))
        Column(Modifier.weight(1f).padding(start = 14.dp)) {
            Text(
                song.title,
                style = MaterialTheme.typography.titleMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                color = if (current && live) c.primary else c.onSurface,
                fontStyle = if (current && !live) FontStyle.Italic else null,
            )
            Text(
                detail,
                style = MaterialTheme.typography.bodySmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                color = c.onSurfaceVariant,
            )
        }
        if (downloaded) Icon(Icons.Downloaded, "Downloaded", Modifier.padding(start = 8.dp).size(16.dp), tint = c.onSurfaceVariant)
        if (song.onPhone) Icon(Icons.Phone, "On this phone", Modifier.padding(start = 8.dp).size(16.dp), tint = c.onSurfaceVariant)
        Text(
            duration(song.durationMs),
            Modifier.width(56.dp),
            style = MaterialTheme.typography.bodySmall,
            color = c.onSurfaceVariant,
            textAlign = TextAlign.End,
        )
        SongMenuButton(song, menu)
    }
}

/** Extra items a list adds to the song menu (e.g. "Remove from queue"). */
data class SongMenu(
    val extra: List<Pair<String, () -> Unit>> = emptyList(),
    val nav: Nav? = null,
)

@Composable
fun SongMenuButton(
    song: Song,
    menu: SongMenu,
) {
    val app = App.app
    val scope = rememberCoroutineScope()
    var open by remember { mutableStateOf(false) }
    var adding by remember { mutableStateOf(false) }
    if (adding) AddToPlaylistDialog(listOf(song)) { adding = false }
    var queueing by remember { mutableStateOf(false) }
    if (queueing) AddToQueueDialog(listOf(song)) { queueing = false }
    val pin = rememberPinner()
    Box {
        Tip("Song options") {
            IconButton({ open = true }) { Icon(Icons.More, "Song options", Modifier.size(20.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
        DropdownMenu(open, { open = false }) {
            // Read only while the menu is open: one database observer per row would add up in a long list.
            val favs by app.store.favorites.collectAsState(emptyList())
            val later by app.store.listenLater.collectAsState(emptyList())
            val isFav = favs.any { it.song == song.id }
            val isLater = later.any { it.song == song.id }
            val close = { open = false }
            MenuItem("Play next", close) { app.playback.playNext(listOf(song)) }
            MenuItem("Add to playing queue", close) { app.playback.addToQueue(listOf(song)) }
            MenuItem("Add to a queue…", close) { queueing = true }
            MenuItem(if (isFav) "Remove from Favorites" else "Add to Favorites", close) { scope.launch { app.store.mark(Store.FAV, song.id, !isFav) } }
            MenuItem(if (isLater) "Remove from Listen Later" else "Listen later", close) { scope.launch { app.store.mark(Store.LATER, song.id, !isLater) } }
            // A phone song stays out of playlists, and is on the phone already (docs/plans/025_phone_local_songs.md).
            if (!song.onPhone) MenuItem("Add to playlist…", close) { adding = true }
            val pins by app.downloads.pins.collectAsState()
            val files by app.downloads.files.collectAsState()
            val key = Downloads.key(Downloads.SONG, song.id.toString())
            if (!song.onPhone && pins.any { it.key == key }) {
                MenuItem("Remove download", close) { scope.launch { app.downloads.unpin(key) } }
            } else if (!song.onPhone && song.id !in files) {
                MenuItem("Download", close) { pin(Downloads.SONG, song.id.toString(), song.title) }
            }
            menu.nav?.let { nav ->
                val catalog by app.catalog.collectAsState()
                val album =
                    catalog.albums
                        .firstOrNull { a -> a.songs.any { it.id == song.id } }
                if (album != null) MenuItem("Go to album", close) { nav.open(Tab.Albums, Page.AlbumPage(album.key)) }
                MenuItem("Go to artist", close) { nav.open(Tab.Artists, Page.ArtistPage(song.artistList.firstOrNull() ?: song.displayArtist)) }
            }
            for ((label, action) in menu.extra) MenuItem(label, close, action)
        }
    }
}

@Composable
fun MenuItem(
    label: String,
    close: () -> Unit,
    action: () -> Unit,
) = DropdownMenuItem({ Text(label) }, {
    close()
    action()
})
