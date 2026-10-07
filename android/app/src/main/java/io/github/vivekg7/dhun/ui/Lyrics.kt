package io.github.vivekg7.dhun.ui

import android.os.SystemClock
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Lyrics
import io.github.vivekg7.dhun.data.Parsed
import io.github.vivekg7.dhun.data.Song
import kotlinx.coroutines.delay

/**
 * Lyrics in place of the cover (docs/plans/015_lyrics.md). Synced lyrics
 * follow the song, the playing line kept in the middle, and a tap on a line
 * plays from there; a tap anywhere else brings the cover back.
 */
@Composable
fun LyricsView(
    song: Song,
    onClose: () -> Unit,
) {
    val state by remember(song.id) { App.app.lyrics.of(song) }.collectAsState(Lyrics.State.Loading)
    val tap = Modifier.fillMaxSize().clickable(interactionSource = null, indication = null, onClick = onClose)
    when (val s = state) {
        Lyrics.State.Loading -> Box(tap)
        Lyrics.State.None -> Message(tap, "No lyrics for this song.")
        Lyrics.State.Offline -> Message(tap, "These lyrics aren't on the phone.\nThey'll show when the server can be reached.")
        is Lyrics.State.Shown -> if (s.lyrics.synced) Synced(song.id, s.lyrics, tap) else Plain(s.lyrics, tap)
    }
}

@Composable
private fun Message(
    modifier: Modifier,
    text: String,
) = Box(modifier, contentAlignment = Alignment.Center) {
    Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center)
}

@Composable
private fun Plain(
    lyrics: Parsed,
    modifier: Modifier,
) {
    LazyColumn(modifier, contentPadding = PaddingValues(vertical = 16.dp)) {
        itemsIndexed(lyrics.lines) { _, l ->
            Text(
                l.text,
                Modifier.fillMaxWidth().padding(vertical = 3.dp),
                style = MaterialTheme.typography.bodyLarge,
                textAlign = TextAlign.Center,
            )
        }
    }
}

@Composable
private fun Synced(
    songId: Long,
    lyrics: Parsed,
    modifier: Modifier,
) {
    val player = App.app.playback.player
    val c = MaterialTheme.colorScheme
    var position by remember { mutableLongStateOf(player.currentPosition) }
    // Polled while shown, playing or not, so a seek while paused moves the line too.
    LaunchedEffect(songId) {
        while (true) {
            position = player.currentPosition
            delay(200)
        }
    }
    val current = lyrics.at(position)
    val list = rememberLazyListState()

    // Scrolling by hand stops the follow for a few seconds, to read ahead.
    var touched by remember { mutableLongStateOf(0L) }
    LaunchedEffect(list) {
        list.interactionSource.interactions.collect {
            if (it is DragInteraction) touched = SystemClock.elapsedRealtime()
        }
    }
    LaunchedEffect(current, touched) {
        val wait = touched + FOLLOW_AGAIN_MS - SystemClock.elapsedRealtime()
        if (wait > 0) delay(wait)
        center(list, current.coerceAtLeast(0))
    }

    BoxWithConstraints(modifier) {
        // Half a screen of space at both ends, so the first and last lines can sit in the middle.
        LazyColumn(Modifier.fillMaxSize(), list, PaddingValues(vertical = maxHeight / 2)) {
            itemsIndexed(lyrics.lines) { i, l ->
                Text(
                    l.text.ifEmpty { "♪" },
                    Modifier
                        .fillMaxWidth()
                        .clickable {
                            position = l.ms
                            player.seekTo(l.ms)
                        }.padding(horizontal = 8.dp, vertical = 6.dp),
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = if (i == current) FontWeight.Bold else FontWeight.Normal,
                    color = if (i == current) c.primary else c.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

/** Scrolls line [i] to the middle of the list. */
private suspend fun center(
    list: LazyListState,
    i: Int,
) {
    fun find() = list.layoutInfo.visibleItemsInfo.firstOrNull { it.index == i }
    val item = find() ?: list.scrollToItem(i).let { find() } ?: return
    val info = list.layoutInfo
    val middle = (info.viewportStartOffset + info.viewportEndOffset) / 2
    list.animateScrollBy((item.offset + item.size / 2 - middle).toFloat())
}

private const val FOLLOW_AGAIN_MS = 4_000L
