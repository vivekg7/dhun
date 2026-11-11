package io.github.vivekg7.dhun.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Playlist
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.bytes
import io.github.vivekg7.dhun.data.songIds

/**
 * The playlists that hold [song], by name (docs/plans/041_search_filters_and_popularity.md).
 * Search asks on every keystroke, and the playlists hold some 18,000
 * entries, so each song's are worked out once per change to the list.
 * Two threads building it at once only build it twice.
 */
fun playlistsOf(
    playlists: List<Playlist>,
    song: Long,
): List<Playlist> {
    val held = membership.takeIf { it.first === playlists }?.second ?: membershipOf(playlists).also { membership = playlists to it }
    return held[song].orEmpty()
}

@Volatile private var membership: Pair<List<Playlist>, Map<Long, List<Playlist>>> = emptyList<Playlist>() to emptyMap()

private fun membershipOf(playlists: List<Playlist>): Map<Long, List<Playlist>> {
    val out = HashMap<Long, MutableList<Playlist>>()
    for (p in playlists.sortedBy { it.name.lowercase() }) for (id in songIds(p.songs).toSet()) out.getOrPut(id) { mutableListOf() } += p
    return out
}

/** A song's tags, file and plays, and the playlists it is in: one screen, read from the menu. */
@Composable
fun SongInfoDialog(
    song: Song,
    nav: Nav?,
    onDismiss: () -> Unit,
) {
    val app = App.app
    val playlists by app.store.playlists.collectAsState()
    val stats by app.store.playStats.collectAsState()
    val inPlaylists = playlistsOf(playlists, song.id)
    val plays = stats.firstOrNull { it.song == song.id }?.count ?: 0
    val rows =
        listOf(
            "Artist" to song.artist,
            "Album" to song.album,
            "Album artist" to song.albumArtist,
            "Composer" to song.composer,
            "Genre" to song.genreList.joinToString(", "),
            "Year" to (if (song.year > 0) "${song.year}" else ""),
            "Length" to duration(song.durationMs),
            "File" to
                dotted(
                    song.format,
                    song.bitrate.takeIf { it > 0 }?.let { "$it kbit/s" },
                    song.sampleRate.takeIf { it > 0 }?.let {
                        if (it % 1000 ==
                            0
                        ) {
                            "${it / 1000} kHz"
                        } else {
                            "${it / 1000.0} kHz"
                        }
                    },
                    bytes(song.size),
                ),
            "Folder" to song.folder,
            "Plays" to "$plays",
            // Spread about evenly over the library: 50 is its median.
            "Popularity" to (if (song.score > 0) "${song.score} of 100" else ""),
        ).filter { it.second.isNotEmpty() }
    AlertDialog(
        onDismiss,
        confirmButton = { TextButton(onDismiss) { Text("Close") } },
        title = { Text(song.title) },
        text = {
            LazyColumn(Modifier.heightIn(max = 480.dp)) {
                items(rows) { (name, value) ->
                    Row(Modifier.padding(vertical = 4.dp)) {
                        Text(name, Modifier.width(104.dp), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        SelectionContainer { Text(value, style = MaterialTheme.typography.bodyMedium) }
                    }
                }
                if (inPlaylists.isNotEmpty()) {
                    item { Text("In playlists", Modifier.padding(top = 16.dp, bottom = 4.dp), style = MaterialTheme.typography.titleSmall) }
                    items(inPlaylists, key = { it.id }) { p -> PlaylistLine(p, nav, onDismiss) }
                }
            }
        },
    )
}

/** The playlists [song] is in, a tap away. */
@Composable
fun InPlaylistsDialog(
    song: Song,
    nav: Nav?,
    onDismiss: () -> Unit,
) {
    val playlists by App.app.store.playlists
        .collectAsState()
    AlertDialog(
        onDismiss,
        confirmButton = { TextButton(onDismiss) { Text("Close") } },
        title = { Text("In playlists") },
        text = {
            Column {
                DialogNote(song.title)
                LazyColumn(Modifier.heightIn(max = 420.dp)) {
                    items(playlistsOf(playlists, song.id), key = { it.id }) { p -> PlaylistLine(p, nav, onDismiss) }
                }
            }
        },
    )
}

@Composable
private fun PlaylistLine(
    p: Playlist,
    nav: Nav?,
    close: () -> Unit,
) = DialogRow(p.name, icon = Icons.Playlist) {
    close()
    nav?.open(Tab.Playlists, Page.PlaylistPage(p.id))
}
