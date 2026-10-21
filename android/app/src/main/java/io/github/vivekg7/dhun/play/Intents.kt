package io.github.vivekg7.dhun.play

import android.app.SearchManager
import android.content.Intent
import android.os.Bundle
import android.provider.MediaStore
import android.widget.Toast
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Catalog
import io.github.vivekg7.dhun.data.Playlist
import io.github.vivekg7.dhun.data.SearchIndex
import io.github.vivekg7.dhun.data.SearchQuery
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.songIds
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/**
 * Requests from outside the app's own screens
 * (docs/plans/038_voice_search_and_assistant.md): the assistant's "play X on
 * Dhun", by intent while the app is closed and through the media session
 * while it runs, and the shortcuts on the icon. A request is resolved
 * against the catalogue on the phone, so it works offline, and plays a new
 * queue named after what was asked; [show] tells the shell which tab to
 * bring up.
 */
class Intents(
    private val app: App,
) {
    enum class Screen { Now, Search }

    private val screens = Channel<Screen>(Channel.CONFLATED)

    /** The tab a request wants on screen, each delivered once. */
    val show: Flow<Screen> = screens.receiveAsFlow()

    /** The Search shortcut was used: the box takes focus, once. */
    val focusSearch = MutableStateFlow(false)

    /** An intent the activity was started or brought forward with; false if it was not a request. */
    fun handle(intent: Intent?): Boolean {
        when (intent?.action) {
            MediaStore.INTENT_ACTION_MEDIA_PLAY_FROM_SEARCH -> {
                play(intent.getStringExtra(SearchManager.QUERY) ?: "", intent.extras)
            }

            SEARCH -> {
                focusSearch.value = true
                screens.trySend(Screen.Search)
            }

            RESUME -> {
                app.scope.launch { resume() }
            }

            SHUFFLE -> {
                app.scope.launch { shuffleAll() }
            }

            else -> {
                return false
            }
        }
        return true
    }

    /**
     * "Play X on Dhun": [query] as heard, [extras] the assistant's hints for
     * what kind of thing X is. Nothing said plays on, or shuffles the library
     * when nothing was playing; nothing found leaves everything as it is.
     */
    fun play(
        query: String,
        extras: Bundle?,
    ) = app.scope.launch {
        val catalog = loaded()
        val pick =
            resolve(
                catalog,
                app.store.playlists.value,
                app.store.played.value
                    .map { it.song }
                    .toSet(),
                query,
                extras?.getString(MediaStore.EXTRA_MEDIA_FOCUS),
            ) {
                extras?.getString(it)?.takeIf { s -> s.isNotBlank() }
            }
        when {
            pick != null -> start(pick)
            query.isBlank() -> resume()
            else -> withContext(Dispatchers.Main) { Toast.makeText(app, "Nothing in the library matches “${query.trim()}”", Toast.LENGTH_LONG).show() }
        }
    }

    private fun start(
        pick: Pick,
        show: Boolean = true,
    ) {
        app.playback.play(pick.name, pick.source, pick.songs, pick.start)
        if (show) screens.trySend(Screen.Now)
    }

    /**
     * Plays on where the listener was: the Resume shortcut, "play something
     * on Dhun", and a headset button pressed with no player running. A cold
     * start is still loading the last queue (Playback's init); with none
     * saved, or none that loads, the library is shuffled instead. [show]
     * brings Now playing up; a headset press leaves the screen alone.
     */
    suspend fun resume(show: Boolean = true) {
        val song =
            if (app.prefs.activeQueue.isEmpty()) {
                null
            } else {
                withTimeoutOrNull(LOAD_WAIT) { app.playback.current.first { it != null } }
            }
        if (song == null) return shuffleAll(show)
        withContext(Dispatchers.Main) { app.playback.play() }
        if (show) screens.trySend(Screen.Now)
    }

    private suspend fun shuffleAll(show: Boolean = true) {
        val songs = loaded().songs
        if (songs.isNotEmpty()) start(Pick("All songs", "all", songs.shuffled()), show)
    }

    /** The catalogue once it holds the library; whatever there is after [LOAD_WAIT]. */
    private suspend fun loaded() = withTimeoutOrNull(LOAD_WAIT) { app.catalog.first { it.byId.isNotEmpty() } } ?: app.catalog.value

    /** What a request plays: a queue's name and source, as the screens pass them to [Playback.play]. */
    class Pick(
        val name: String,
        val source: String,
        val songs: List<Song>,
        val start: Int = 0,
    )

    companion object {
        const val SEARCH = "io.github.vivekg7.dhun.SEARCH"
        const val RESUME = "io.github.vivekg7.dhun.RESUME"
        const val SHUFFLE = "io.github.vivekg7.dhun.SHUFFLE"

        // How long a cold start may take to load the catalogue and the last queue.
        private const val LOAD_WAIT = 15_000L

        // MediaStore's content types, as the assistant's focus extra names them.
        private const val ARTIST = "vnd.android.cursor.item/artist"
        private const val ALBUM = "vnd.android.cursor.item/album"
        private const val GENRE = "vnd.android.cursor.item/genre"
        private const val PLAYLIST = "vnd.android.cursor.item/playlist"
        private const val TITLE = "vnd.android.cursor.item/audio"

        /**
         * What a spoken request plays. A [focus] hint names the field, and
         * [hint] gives its value by the extra's name; without one, [query]
         * goes through every section the Search tab has, and the best match
         * of all wins, a tie going to the earlier section, as on the tab. A
         * book or a show starts where the listener is, as its page does.
         */
        fun resolve(
            catalog: Catalog,
            playlists: List<Playlist>,
            played: Set<Long>,
            query: String,
            focus: String?,
            hint: (String) -> String?,
        ): Pick? {
            val playlistIndex = SearchIndex.names(playlists, "playlist") { it.name }

            fun <T> SearchIndex<T>.best(text: String?) = text?.takeIf { it.isNotBlank() }?.let { search(SearchQuery(it)).firstOrNull()?.first }

            fun songs(
                index: SearchIndex<Song>,
                text: String,
            ): Pick? =
                index
                    .search(SearchQuery(text))
                    .map { it.first }
                    .takeIf { it.isNotEmpty() }
                    ?.let { Pick(text.trim(), "search", it) }

            fun playlist(p: Playlist) = Pick(p.name, "playlist:${p.id}", catalog.songsOf(songIds(p.songs)))
            val artist = hint(MediaStore.EXTRA_MEDIA_ARTIST)
            val album = hint(MediaStore.EXTRA_MEDIA_ALBUM)
            val named = listOfNotNull(hint(MediaStore.EXTRA_MEDIA_TITLE), artist, album).joinToString(" ")
            val hinted =
                when (focus) {
                    ARTIST -> catalog.artistIndex.best(artist)?.let { Pick(it.name, "artist:${it.name}", it.songs) }
                    ALBUM -> catalog.albumIndex.best(listOfNotNull(album, artist).joinToString(" "))?.let { Pick(it.name, "album:${it.key}", it.songs) }
                    GENRE -> catalog.genreIndex.best(hint(MediaStore.EXTRA_MEDIA_GENRE))?.let { Pick(it.name, "genre:${it.name}", it.songs) }
                    PLAYLIST -> playlistIndex.best(hint(MediaStore.EXTRA_MEDIA_PLAYLIST))?.let(::playlist)
                    TITLE -> songs(catalog.songIndex, named)
                    else -> null
                }
            if (hinted != null) return hinted
            val q = SearchQuery(query)
            if (q.isEmpty) return null

            fun <T> SearchIndex<T>.top(pick: (T) -> Pick?) = search(q).firstOrNull()?.let { (item, score) -> score to { pick(item) } }
            val sections =
                listOfNotNull(
                    catalog.artistIndex.top { Pick(it.name, "artist:${it.name}", it.songs) },
                    catalog.albumIndex.top { Pick(it.name, "album:${it.key}", it.songs) },
                    catalog.genreIndex.top { Pick(it.name, "genre:${it.name}", it.songs) },
                    catalog.folderIndex.top { Pick(it.name, "folder:${it.path}", it.all) },
                    playlistIndex.top(::playlist),
                    catalog.bookIndex.top { b ->
                        val start = b.items.firstOrNull { it.id !in played } ?: b.items.first()
                        Pick(b.title, "book:${b.title}", b.items, b.items.indexOf(start))
                    },
                    catalog.showIndex.top { s ->
                        val start = s.items.firstOrNull { it.id !in played } ?: s.items.first()
                        Pick(s.title, "show:${s.title}", s.items.drop(s.items.indexOf(start)).filter { it == start || it.id !in played })
                    },
                    catalog.episodeIndex.top { songs(catalog.episodeIndex, query) },
                    catalog.songIndex.top { songs(catalog.songIndex, query) },
                )
            return sections.minByOrNull { it.first }?.second?.invoke()
        }
    }
}
