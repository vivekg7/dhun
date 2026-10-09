package io.github.vivekg7.dhun.data

import android.app.ForegroundServiceStartNotAllowedException
import android.content.Intent
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.Environment
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.play.DownloadService
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.Request
import java.io.File
import java.io.IOException
import java.io.RandomAccessFile

/**
 * Downloads (docs/plans/012_downloads.md). The user pins things (an album,
 * a playlist, Favorites); the pins are turned into songs again whenever the
 * library or the lists change, and the files on disk follow: new songs are
 * fetched, songs no pin covers any more are deleted.
 */
class Downloads(
    private val app: App,
) {
    private val dao get() = app.db.dao()

    val pins: StateFlow<List<Pin>> = dao.pins().stateIn(app.scope, SharingStarted.Eagerly, emptyList())

    /** What is on disk, by song. */
    val files: StateFlow<Map<Long, Download>> =
        dao.downloads().map { rows -> rows.associateBy { it.song } }.stateIn(app.scope, SharingStarted.Eagerly, emptyMap())

    /** The songs the pins cover now ([covered]). */
    val wanted: StateFlow<List<Song>?> =
        combine(pins, app.catalog, app.store.playlists, app.store.favorites, app.store.listenLater, ::covered)
            .stateIn(app.scope, SharingStarted.Eagerly, null)

    enum class State { Idle, Downloading, NoNetwork, WaitingForWifi, Full, NoSpace, Failed }

    data class Status(
        val state: State = State.Idle,
        /** Songs covered by pins, and how many of them are on disk. */
        val wanted: Int = 0,
        val done: Int = 0,
        val usedBytes: Long = 0,
        /** Full: how much more space the rest needs. Failed: the error. */
        val needBytes: Long = 0,
        val error: String = "",
        val current: Song? = null,
        val progress: Float = 0f,
    )

    private val _status = MutableStateFlow(Status())
    val status: StateFlow<Status> = _status

    private val wake = Channel<Unit>(Channel.CONFLATED)

    init {
        app.scope.launch { wanted.collect { poke() } }
        app.scope.launch {
            for (unit in wake) {
                try {
                    run()
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    fail(e)
                }
            }
        }
    }

    /** Looks at the pins and the files again: after a change, a new network, or the app coming back. */
    fun poke() {
        wake.trySend(Unit)
    }

    suspend fun pin(
        kind: String,
        ref: String,
        name: String,
    ) = dao.putPin(Pin(key(kind, ref), kind, ref, name, System.currentTimeMillis()))

    suspend fun unpin(key: String) = dao.deletePin(key)

    /** The local file for a song, if it is downloaded; called by the player when it opens a song. */
    fun file(song: Long): File? = files.value[song]?.let { File(it.path) }?.takeIf { it.exists() }

    /** On a sign-out that does not keep them. */
    suspend fun deleteAll() =
        withContext(Dispatchers.IO) {
            for (d in files.value.values) File(d.path).delete()
            app.getExternalFilesDirs(DIR).filterNotNull().forEach { it.deleteRecursively() }
        }

    /** A kept file that is not the song the server has under its ID ([Sync]). */
    suspend fun drop(d: Download) {
        withContext(Dispatchers.IO) { File(d.path).delete() }
        dao.deleteDownload(d.song)
        dao.deletePin(key(SONG, d.song.toString()))
    }

    /**
     * Someone else signs in to downloads kept from another account
     * (docs/plans/026_without_an_account.md): every file stays, as a pin of
     * its own song, until they remove it. Pins of the other person's
     * playlists and lists go; album, folder, artist and genre pins are the
     * library's and stay.
     */
    suspend fun adopt() {
        val songs = app.catalog.value.byId
        for (d in dao.downloads().first()) {
            val ref = d.song.toString()
            dao.putPin(Pin(key(SONG, ref), SONG, ref, songs[d.song]?.title.orEmpty(), d.at))
        }
        for (p in dao.pins().first()) if (p.kind == PLAYLIST || p.kind == LIST) dao.deletePin(p.key)
    }

    private suspend fun run() {
        val songs = wanted.value ?: return
        val want = songs.associateBy { it.id }
        // Files nothing covers any more: removed from a playlist, unpinned, gone from the library.
        // Not while kept through a sign-out: their pins' playlists and marks are not here yet.
        val checked = if (app.prefs.keptFrom.isEmpty()) files.value.values else emptyList()
        for (d in checked) {
            if (d.song !in want) {
                withContext(Dispatchers.IO) { File(d.path).delete() }
                dao.deleteDownload(d.song)
            } else if (mounted(File(d.path)) && !File(d.path).exists()) {
                // Deleted behind our back (a file manager, a cleaner): fetch it again.
                // An SD card taken out is not "deleted": those wait for the card.
                dao.deleteDownload(d.song)
            }
        }
        val have = dao.downloads().first().associateBy { it.song }
        val todo = songs.filter { s -> have[s.id].let { it == null || (s.size > 0 && it.size != s.size) } }
        var used = have.values.filter { it.song in want }.sumOf { it.size }
        // A part kept for resuming whose song is no longer wanted.
        val resumable = todo.map { it.id }.toSet()
        dir()?.listFiles { f -> f.name.endsWith(".part") && f.name.substringBefore('.').toLongOrNull() !in resumable }?.forEach { it.delete() }
        update(State.Idle, songs.size, used, have)
        if (app.prefs.token.isEmpty()) return
        keepLyrics(songs.filter { it.id in have })
        if (todo.isEmpty()) return

        for ((i, s) in todo.withIndex()) {
            // Something changed while the last file was fetched (a new pin, an
            // edited playlist): start over, so the newest pin goes first.
            if (i > 0 && wake.tryReceive().isSuccess) return poke()
            when {
                !hasNetwork() -> return update(State.NoNetwork, songs.size, used, have)
                app.prefs.wifiOnly && onMobileData() -> return update(State.WaitingForWifi, songs.size, used, have)
            }
            val limit = app.prefs.downloadLimitGb * GB
            val replacing = have[s.id]?.size ?: 0
            if (limit > 0 && used - replacing + s.size > limit) {
                val rest = todo.drop(i).sumOf { it.size }
                return update(State.Full, songs.size, used, have, need = used + rest - limit)
            }
            val dir = dir() ?: return update(State.NoSpace, songs.size, used, have)
            if (dir.usableSpace < s.size + RESERVE) {
                return update(State.NoSpace, songs.size, used, have, need = s.size + RESERVE - dir.usableSpace)
            }
            startService()
            _status.value = _status.value.copy(state = State.Downloading, current = s, progress = 0f)
            // A song in the song cache moves here rather than being fetched again.
            val file = app.cache.promote(s, dir) ?: fetch(s, dir) ?: continue
            have[s.id]?.let { old -> if (old.path != file.path) File(old.path).delete() }
            val row = Download(s.id, file.path, s.size, System.currentTimeMillis())
            dao.putDownload(row)
            used += s.size - replacing
            _status.value = _status.value.copy(done = _status.value.done + 1, usedBytes = used)
        }
        val done = dao.downloads().first().associateBy { it.song }
        keepLyrics(songs.filter { it.id in done })
        update(State.Idle, songs.size, used, done)
    }

    /** A downloaded song's lyrics are kept with it (docs/plans/015_lyrics.md); a failure here never stops the files. */
    private suspend fun keepLyrics(songs: List<Song>) {
        if (!hasNetwork()) return
        try {
            app.lyrics.keep(songs)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            // Tried again on the next run.
        }
    }

    /**
     * Fetches one song to `<id>.<ext>`, through a `.part` file so a
     * half-written file is never played; null if the server no longer has
     * it. A part left by a broken try is carried on with a range request
     * rather than fetched again from the start
     * (docs/plans/019_networking_and_caching.md): a large FLAC on a weak
     * network might otherwise never finish.
     */
    private suspend fun fetch(
        s: Song,
        dir: File,
    ): File? =
        withContext(Dispatchers.IO) {
            val ext =
                s.path
                    .substringAfterLast('.', "")
                    .lowercase()
                    .ifEmpty { s.format }
            val file = File(dir, "${s.id}.$ext")
            val part = File(dir, "${s.id}.$ext.part")
            var got = part.length()
            val req =
                Request
                    .Builder()
                    .url(app.api.streamUrl(s.id))
                    .apply { if (got > 0) header("Range", "bytes=$got-") }
                    .build()
            app.api.songs.newCall(req).execute().use { res ->
                when {
                    res.code == 404 || res.code == 410 -> {
                        part.delete()
                        return@withContext null
                    }

                    // The part is longer than the file: the file changed. Start again.
                    res.code == 416 -> {
                        part.delete()
                        throw IOException("${s.title}: the file changed on the server")
                    }

                    !res.isSuccessful -> {
                        throw ApiException(res.code, "HTTP ${res.code}")
                    }

                    // The server ignored the range: start again.
                    res.code == 200 -> {
                        got = 0
                    }
                }
                val body = res.body
                val total = body.contentLength().takeIf { it >= 0 }?.let { it + got } ?: s.size
                var shown = got
                RandomAccessFile(part, "rw").use { out ->
                    out.setLength(got)
                    out.seek(got)
                    body.byteStream().use { input ->
                        val buf = ByteArray(64 * 1024)
                        while (true) {
                            val n = input.read(buf)
                            if (n < 0) break
                            out.write(buf, 0, n)
                            got += n
                            if (got - shown > 512 * 1024 && total > 0) {
                                shown = got
                                _status.value = _status.value.copy(progress = got.toFloat() / total)
                            }
                        }
                    }
                }
                // Against what the server sent: the song list may be older than the file.
                if (total > 0 && got != total) {
                    part.delete()
                    throw IOException("${s.title}: got $got of $total bytes")
                }
                if (!part.renameTo(file)) throw IOException("Could not save ${file.name}")
            }
            file
        }

    private fun fail(e: Exception) {
        _status.value = _status.value.copy(state = State.Failed, error = e.message ?: e.toString(), current = null)
        // A dropped connection mid-file: try again shortly.
        app.scope.launch {
            delay(RETRY_MS)
            poke()
        }
    }

    private fun update(
        state: State,
        wanted: Int,
        used: Long,
        have: Map<Long, Download>,
        need: Long = 0,
    ) {
        val done = this.wanted.value?.count { it.id in have } ?: 0
        _status.value = Status(state, wanted, done, used, need)
    }

    /**
     * The SD card when there is one (docs/plans/007_client_architecture.md),
     * else the phone. No permission is needed for either. The song cache
     * asks for its folder here too, to be on the same storage.
     */
    fun dir(name: String = DIR): File? {
        val dirs = app.getExternalFilesDirs(name).filterNotNull().filter(::mounted)
        return (dirs.firstOrNull { Environment.isExternalStorageRemovable(it) } ?: dirs.firstOrNull())?.also { it.mkdirs() }
    }

    /** Where downloads go: "SD card" or "phone", for the settings. */
    fun location(): String {
        val d = dir() ?: return "nowhere: no storage is available"
        return if (Environment.isExternalStorageRemovable(d)) "the SD card" else "the phone"
    }

    private fun mounted(f: File) = runCatching { Environment.getExternalStorageState(f) == Environment.MEDIA_MOUNTED }.getOrDefault(false)

    private fun caps(): NetworkCapabilities? {
        val cm = app.getSystemService(ConnectivityManager::class.java)
        return cm.getNetworkCapabilities(cm.activeNetwork)
    }

    private fun hasNetwork() = caps()?.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) == true

    /** Through a VPN (Tailscale), Android reports the transport of the network underneath. */
    fun onMobileData() = caps()?.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) == true

    /**
     * The foreground service keeps the process alive and shows progress.
     * Android 12+ refuses to start one from the background; the downloads
     * then run while the app is alive anyway, and the rest waits for the
     * next start ([App] pokes on every return to the foreground).
     */
    private fun startService() {
        if (DownloadService.running) return
        try {
            app.startForegroundService(Intent(app, DownloadService::class.java))
        } catch (_: ForegroundServiceStartNotAllowedException) {
        }
    }

    companion object {
        const val SONG = "song"
        const val ALBUM = "album"
        const val FOLDER = "folder"
        const val ARTIST = "artist"
        const val GENRE = "genre"
        const val PLAYLIST = "playlist"

        /** Favorites or Listen Later; [Pin.ref] is the [Store] mark kind. */
        const val LIST = "list"

        private const val DIR = "downloads"
        const val GB = 1L shl 30

        /** Left free on the card, so a full card does not break the rest of the phone. */
        private const val RESERVE = 200L shl 20
        private const val RETRY_MS = 30_000L

        fun key(
            kind: String,
            ref: String,
        ) = "$kind:$ref"
    }
}

/**
 * The songs [pins] cover, newest pin first so what was just tapped starts
 * first; null while the NAS songs are not read, because an empty catalogue
 * would read as "delete every download". Phone songs are on the phone
 * already, so never wanted.
 */
fun covered(
    pins: List<Pin>,
    c: Catalog,
    playlists: List<Playlist>,
    favs: List<Mark>,
    later: List<Mark>,
): List<Song>? {
    if (!c.hasNas) return null
    val ids = LinkedHashSet<Long>()
    for (p in pins.sortedByDescending { it.at }) {
        when (p.kind) {
            Downloads.SONG -> {
                p.ref.toLongOrNull()?.let { ids += it }
            }

            Downloads.ALBUM -> {
                c.albums
                    .firstOrNull { it.key == p.ref }
                    ?.songs
                    ?.forEach { ids += it.id }
            }

            Downloads.FOLDER -> {
                c.folders[p.ref]?.allSongs()?.forEach { ids += it.id }
            }

            Downloads.ARTIST -> {
                c.artists
                    .firstOrNull { it.name.equals(p.ref, ignoreCase = true) }
                    ?.songs
                    ?.forEach { ids += it.id }
            }

            Downloads.GENRE -> {
                c.genres
                    .firstOrNull { it.name.equals(p.ref, ignoreCase = true) }
                    ?.songs
                    ?.forEach { ids += it.id }
            }

            Downloads.PLAYLIST -> {
                playlists.firstOrNull { it.id.toString() == p.ref }?.let { ids += songIds(it.songs) }
            }

            Downloads.LIST -> {
                (if (p.ref == Store.FAV) favs else later).forEach { ids += it.song }
            }
        }
    }
    return c.songsOf(ids.toList()).filter { !it.onPhone }
}

/** "1.2 GB", "340 MB" */
fun bytes(n: Long): String =
    when {
        n >= Downloads.GB -> "%.1f GB".format(n.toDouble() / Downloads.GB)
        n >= 1L shl 20 -> "${n shr 20} MB"
        else -> "${n shr 10} KB"
    }
