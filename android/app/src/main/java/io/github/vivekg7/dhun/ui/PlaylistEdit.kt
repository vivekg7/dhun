package io.github.vivekg7.dhun.ui

import android.content.Context
import android.widget.Toast
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Playlist
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.songIds
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

/**
 * Editing playlists (docs/plans/013_playlist_editing.md). Only the user's
 * own: shared playlists stay read-only in the app, by the owner's choice.
 */
fun Playlist.editable() = !shared

/** The server keeps these names for its own lists. */
private val RESERVED = setOf("favorites", "listen later")

/** A name for a queue or a playlist; null when cancelled. Blank and reserved names are not accepted. */
@Composable
fun NameDialog(
    title: String,
    initial: String,
    confirm: String,
    onDone: (String?) -> Unit,
) {
    // The old name selected and the keyboard up, so typing replaces it.
    var text by remember { mutableStateOf(TextFieldValue(initial, TextRange(0, initial.length))) }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    val name = text.text.trim()
    val ok = name.isNotEmpty() && name.lowercase() !in RESERVED
    AlertDialog(
        onDismissRequest = { onDone(null) },
        title = { Text(title) },
        text = { OutlinedTextField(text, { text = it }, Modifier.focusRequester(focus), singleLine = true) },
        confirmButton = { TextButton({ onDone(name) }, enabled = ok) { Text(confirm) } },
        dismissButton = { TextButton({ onDone(null) }) { Text("Cancel") } },
    )
}

/** A new playlist of [songs], with a note saying so. */
fun savePlaylist(
    context: Context,
    name: String,
    songs: List<Song>,
) {
    val app = App.app
    app.scope.launch {
        app.store.createPlaylist(name, songs.map { it.id })
        note(context, "Saved “$name” · ${songs.distinctBy { it.id }.size} songs")
    }
}

private fun add(
    context: Context,
    p: Playlist,
    songs: List<Song>,
) {
    val app = App.app
    app.scope.launch {
        val skipped = app.store.addToPlaylist(p, songs.map { it.id })
        val added = songs.distinctBy { it.id }.size - skipped
        note(
            context,
            when {
                added == 0 -> if (skipped == 1) "Already in “${p.name}”" else "All $skipped already in “${p.name}”"
                skipped > 0 -> "Added $added to “${p.name}” · $skipped already there"
                else -> "Added $added to “${p.name}”"
            },
        )
    }
}

private suspend fun note(
    context: Context,
    text: String,
) = kotlinx.coroutines.withContext(Dispatchers.Main) { Toast.makeText(context, text, Toast.LENGTH_SHORT).show() }

/**
 * Add [songs] to one of your playlists, or to a new one ([newName] is
 * offered for it). Songs already in it are skipped.
 */
@Composable
fun AddToPlaylistDialog(
    songs: List<Song>,
    newName: String = "",
    onDismiss: () -> Unit,
) {
    val app = App.app
    val context = LocalContext.current
    val playlists by app.store.playlists.collectAsState(emptyList())
    var naming by remember { mutableStateOf(false) }
    if (naming) {
        NameDialog("New playlist", newName, "Create") { name ->
            if (name != null) savePlaylist(context, name, songs)
            onDismiss()
        }
        return
    }
    val mine = playlists.filter { it.editable() }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (songs.size == 1) "Add “${songs[0].title}” to" else "Add ${songs.size} songs to") },
        text = {
            LazyColumn(Modifier.heightIn(max = 360.dp)) {
                item { DialogRow("New playlist", icon = Icons.Add) { naming = true } }
                items(mine, key = { it.id }) { p ->
                    val count = songIds(p.songs).count { it != 0L }
                    DialogRow(p.name, icon = Icons.Playlist, detail = "$count") {
                        add(context, p, songs)
                        onDismiss()
                    }
                }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onDismiss) { Text("Cancel") } },
    )
}
