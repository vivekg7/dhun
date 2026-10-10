package io.github.vivekg7.dhun.ui

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Catalog
import io.github.vivekg7.dhun.data.Downloads
import io.github.vivekg7.dhun.data.Downloads.State
import io.github.vivekg7.dhun.data.Pin
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.bytes
import kotlinx.coroutines.launch

/** What a page's Download button keeps on the phone (docs/plans/012_downloads.md). */
data class DownloadTarget(
    val kind: String,
    val ref: String,
    val name: String,
    val songs: List<Song>,
) {
    val key get() = Downloads.key(kind, ref)
}

/**
 * Pins something for download. The first time, it also asks to show
 * notifications: from Android 13 the progress notification is hidden
 * without that.
 */
@Composable
fun rememberPinner(): (kind: String, ref: String, name: String) -> Unit {
    val app = App.app
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { }
    return { kind, ref, name ->
        scope.launch { app.downloads.pin(kind, ref, name) }
        // Android 12 shows notifications without asking.
        if (Build.VERSION.SDK_INT >= 33 &&
            !app.prefs.askedNotifications &&
            context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            app.prefs.askedNotifications = true
            ask.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }
}

/** Download, or how much of it is on the phone; tapping a pinned one offers to remove it. */
@Composable
fun DownloadButton(t: DownloadTarget) {
    val app = App.app
    val pins by app.downloads.pins.collectAsState()
    val files by app.downloads.files.collectAsState()
    val pin = rememberPinner()
    var confirm by remember { mutableStateOf(false) }
    val pinned = pins.any { it.key == t.key }
    val done = t.songs.count { it.id in files }
    // A tap on a download removes it, so that is what it is called.
    val label = if (pinned) "Remove download" else "Download"
    Tip(label) {
        FilledTonalButton(
            { if (pinned) confirm = true else pin(t.kind, t.ref, t.name) },
            contentPadding = PaddingValues(horizontal = 14.dp),
        ) {
            Icon(if (pinned && done == t.songs.size) Icons.Downloaded else Icons.Download, label, Modifier.size(18.dp))
            if (pinned && done < t.songs.size) {
                Spacer(Modifier.width(6.dp))
                Text("$done/${t.songs.size}")
            }
        }
    }
    if (confirm) RemoveDialog(t.name, { confirm = false }) { app.scope.launch { app.downloads.unpin(t.key) } }
}

@Composable
private fun RemoveDialog(
    name: String,
    onDismiss: () -> Unit,
    onRemove: () -> Unit,
) = AlertDialog(
    onDismiss,
    confirmButton = {
        TextButton({
            onDismiss()
            onRemove()
        }) { Text("Remove") }
    },
    dismissButton = { TextButton(onDismiss) { Text("Keep") } },
    title = { Text("Remove download?") },
    text = { Text("“$name” stays in the library. Its songs are deleted from this phone, unless something else you downloaded has them too.") },
)

/** One line on what the downloads are doing, for the Playlists card and the Downloads page. */
fun describe(s: Downloads.Status): String {
    val used = "${bytes(s.usedBytes)} · ${s.done} of ${s.wanted} songs"
    return when (s.state) {
        State.Idle -> if (s.wanted == 0) "Nothing downloaded yet" else used
        State.Downloading -> "Downloading ${(s.done + 1).coerceAtMost(s.wanted)} of ${s.wanted}"
        State.NoNetwork -> "Waiting for a network · $used"
        State.WaitingForWifi -> "Waiting for Wi-Fi · $used"
        State.Full -> "Stopped: the limit needs ${bytes(s.needBytes)} more"
        State.NoSpace -> "Stopped: the storage needs ${bytes(s.needBytes)} more free space"
        State.Failed -> "Will retry: ${s.error}"
    }
}

private fun kindLabel(p: Pin) =
    when (p.kind) {
        Downloads.SONG -> "Song"
        Downloads.ALBUM -> "Album"
        Downloads.FOLDER -> "Folder"
        Downloads.ARTIST -> "Artist"
        Downloads.GENRE -> "Genre"
        Downloads.PLAYLIST -> "Playlist"
        else -> "List"
    }

private val STOPPED = setOf(State.Full, State.NoSpace, State.Failed)

/** What is downloaded, and what the downloader is doing. */
@Composable
fun DownloadsScreen(
    nav: Nav,
    onBack: () -> Unit,
) {
    val app = App.app
    val status by app.downloads.status.collectAsState()
    val pins by app.downloads.pins.collectAsState()
    val files by app.downloads.files.collectAsState()
    val catalog by app.catalog.collectAsState()
    val songs = remember(files, catalog) { catalog.songs.filter { it.id in files } }
    var removing by remember { mutableStateOf<Pin?>(null) }
    val limit = app.prefs.downloadLimitGb
    val searchable = pins.size + songs.size > SEARCH_OVER
    var query by rememberSaveable { mutableStateOf("") }
    val q = if (searchable) Catalog.fold(query) else ""
    val shownPins = remember(pins, q) { if (q.isEmpty()) pins else pins.filter { Catalog.fold(it.name).contains(q) } }
    // A song found still plays every download from it.
    val hits = remember(songs, q) { songs.withIndex().filter { matches(it.value, q) } }
    Column(Modifier.fillMaxSize()) {
        LazyColumn(Modifier.weight(1f)) {
            item {
                PageHeader(
                    "Downloads",
                    "${bytes(status.usedBytes)} of ${if (limit == 0) "no limit" else "$limit GB"} on ${app.downloads.location()}",
                    onBack,
                    { playList(nav, "Downloads", "downloads", songs, 0) },
                    songs = songs,
                ) { shuffleList(nav, "Downloads", "downloads", songs) }
            }
            // Idle, the header already says it all.
            if (status.state != State.Idle) {
                item {
                    Text(
                        describe(status),
                        Modifier.padding(horizontal = 20.dp, vertical = 4.dp),
                        style = MaterialTheme.typography.bodyMedium,
                        color = if (status.state in STOPPED) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            if (status.state == State.Full) {
                item { TextButton({ nav.settings = SettingsPage.Downloads }, Modifier.padding(start = 8.dp)) { Text("Change the limit in Settings") } }
            }
            if (q.isEmpty() || shownPins.isNotEmpty()) item { SectionLabel("Kept in step with the library") }
            items(shownPins, key = { it.key }) { p ->
                Row(
                    Modifier.padding(start = 20.dp, end = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(p.name, style = MaterialTheme.typography.bodyLarge, maxLines = 1)
                        Text(kindLabel(p), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    Tip("Remove download") { IconButton({ removing = p }) { Icon(Icons.Close, "Remove download", Modifier.size(20.dp)) } }
                }
            }
            if (pins.isEmpty()) item { Empty("Use Download on an album, a playlist or a folder to keep it on this phone.") }
            if (hits.isNotEmpty()) item { SectionLabel("Songs") }
            items(hits, key = { (_, s) -> s.id }) { (i, s) ->
                SongRow(s, onClick = { playList(nav, "Downloads", "downloads", songs, i) }, menu = SongMenu(nav = nav))
            }
            if (q.isNotEmpty() && shownPins.isEmpty() && hits.isEmpty()) item { Empty("Nothing here matches “${query.trim()}”") }
        }
        if (searchable) SearchField(query, { query = it }, "Search in Downloads…", bottom = true)
    }
    removing?.let { p -> RemoveDialog(p.name, { removing = null }) { app.scope.launch { app.downloads.unpin(p.key) } } }
}
