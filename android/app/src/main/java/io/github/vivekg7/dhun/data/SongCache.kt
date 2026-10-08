package io.github.vivekg7.dhun.data

import androidx.media3.common.C
import androidx.media3.common.Player
import io.github.vivekg7.dhun.App
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.Request
import java.io.File
import java.io.FileNotFoundException
import java.io.IOException
import java.io.InterruptedIOException
import java.io.RandomAccessFile
import java.util.concurrent.ConcurrentHashMap

/**
 * The song cache (docs/plans/019_networking_and_caching.md): original files,
 * fetched whole at the network's full speed, so a song is on the phone
 * seconds after it starts and a later gap in the network does not matter.
 * It keeps the song playing (every song played is kept) and the next two of
 * the queue, or the next ten on Wi-Fi, while playing. The player reads a
 * file while it is still arriving ([open]).
 *
 * Plain files beside Downloads, not Media3's SimpleCache, so a cached song
 * becomes a download by a rename (docs/plans/007_client_architecture.md);
 * a file's modification time is when it was last played, for dropping the
 * oldest at the limit. Never holds a downloaded song.
 */
class SongCache(
    private val app: App,
) {
    /** A song being fetched, as the player reading it sees it. */
    class Entry(
        val song: Long,
        @Volatile var file: File,
    ) {
        private val lock = Object()

        @Volatile var written = 0L

        /** The whole file's length, once the server has said; else -1. */
        @Volatile var total = -1L

        @Volatile var done = false

        /** The last try failed and the fetcher is waiting to try again: a reader that has caught up says so. */
        @Volatile var failing = false

        /** The server no longer has the song: final, unlike [failing]. */
        @Volatile var gone = false

        fun changed() = synchronized(lock) { lock.notifyAll() }

        /** Blocks until a byte past [pos] has arrived, and returns how many have; or the end, when the file is whole. */
        fun awaitPast(pos: Long): Long {
            synchronized(lock) {
                while (true) {
                    if (gone) throw FileNotFoundException("Song $song is no longer on the server")
                    if (written > pos || done) return written
                    // An error lets the player retry and say it is waiting for the network.
                    if (failing) throw IOException("Waiting for the network")
                    try {
                        lock.wait(250)
                    } catch (e: InterruptedException) {
                        // The player cancelled the load: a seek, a skip, a release.
                        Thread.currentThread().interrupt()
                        throw InterruptedIOException()
                    }
                }
            }
        }
    }

    private val entries = ConcurrentHashMap<Long, Entry>()
    private val wake = Channel<Unit>(Channel.CONFLATED)

    /** Set by [poke]: what to fetch may have changed, so a fetch under way checks it is still wanted. */
    @Volatile private var replan = false

    private val _used = MutableStateFlow(0L)

    /** Bytes in the cache, for Settings. */
    val used: StateFlow<Long> = _used

    private val _songs = MutableStateFlow(emptySet<Long>())

    /** Songs whole in the cache: offline, they play, so the lists do not dim them. */
    val songs: StateFlow<Set<Long>> = _songs

    init {
        app.scope.launch(Dispatchers.IO) {
            measure()
            for (unit in wake) {
                try {
                    run()
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    // A full card, a vanished folder: tried again on the next poke.
                }
            }
        }
    }

    /** Looks again at what to fetch: the song or the queue changed, play started, the network changed, the limit changed. */
    fun poke() {
        replan = true
        wake.trySend(Unit)
    }

    /** A cached song's whole file, marked as just played; null if it is not all here. */
    fun complete(song: Long): File? {
        val s = app.catalog.value.byId[song] ?: return null
        val dir = dir() ?: return null
        return File(dir, name(s)).takeIf { it.exists() }?.also { it.setLastModified(System.currentTimeMillis()) }
    }

    /**
     * Opens a song not yet whole, to read from [pos] while it arrives; the
     * fetcher is told, as the song the player wants is first in its plan.
     * Null when the cache is off, or [pos] is far past what has arrived (a
     * seek ahead), which the stream serves sooner.
     */
    fun open(
        song: Long,
        pos: Long,
    ): Pair<Entry, RandomAccessFile>? {
        if (app.prefs.cacheLimitGb == 0) return null
        val s = app.catalog.value.byId[song] ?: return null
        val dir = dir() ?: return null
        val e =
            entries.computeIfAbsent(song) {
                poke()
                Entry(song, File(dir, name(s) + ".part")).also { it.written = it.file.length() }
            }
        synchronized(e) {
            if (e.done || e.gone || pos > e.written + SEEK_AHEAD) return null
            // Finished just now, between the player's look for a whole file and this.
            if (File(dir, name(s)).exists()) return null.also { entries.remove(song, e) }
            e.file.createNewFile()
            // Opened under the lock that renames it: the descriptor stays good after the rename.
            return e to RandomAccessFile(e.file, "r")
        }
    }

    /**
     * Moves a cached song into [dir], as a download (REQUIREMENTS: never
     * fetched twice); null if it is not all here.
     */
    fun promote(
        s: Song,
        dir: File,
    ): File? {
        val from = complete(s.id) ?: return null
        if (s.size > 0 && from.length() != s.size) return null
        val to = File(dir, from.name)
        return to.takeIf { from.renameTo(it) }?.also { measure() }
    }

    /** On sign-out, with the downloads. */
    suspend fun deleteAll() =
        withContext(Dispatchers.IO) {
            app.getExternalFilesDirs(DIR).filterNotNull().forEach { it.deleteRecursively() }
            entries.clear()
            measure()
        }

    private suspend fun run() {
        replan = false
        val dir = dir() ?: return
        val limit = app.prefs.cacheLimitGb * Downloads.GB
        val plan = if (limit > 0) plan() else emptyList()
        trim(dir, limit, plan)
        for (s in plan) {
            if (replan) return
            if (File(dir, name(s)).exists()) continue
            if (!fetch(s, dir)) return
            trim(dir, limit, plan)
        }
    }

    /** What to keep now, first things first: the song playing, then those after it while playing. */
    private suspend fun plan(): List<Song> =
        withContext(Dispatchers.Main) {
            val player = app.playback.player
            val t = player.currentTimeline
            val first = player.currentMediaItemIndex
            if (t.isEmpty || first >= t.windowCount) return@withContext emptyList()
            val ahead =
                if (!player.playWhenReady) {
                    0
                } else if (app.downloads.onMobileData()) {
                    AHEAD_MOBILE
                } else {
                    AHEAD_WIFI
                }
            // Repeating one song, the next is still the next in the queue.
            val repeat = if (player.repeatMode == Player.REPEAT_MODE_ALL) Player.REPEAT_MODE_ALL else Player.REPEAT_MODE_OFF
            val order = mutableListOf(first)
            var i = first
            while (order.size <= ahead) {
                i = t.getNextWindowIndex(i, repeat, player.shuffleModeEnabled)
                if (i == C.INDEX_UNSET || i in order) break
                order += i
            }
            order
                .mapNotNull { app.catalog.value.byId[player.getMediaItemAt(it).mediaId.toLongOrNull()] }
                .filter { app.downloads.file(it.id) == null }
        }

    /**
     * Fetches [s] into the cache, resuming a part already here. Retries a
     * failure while the song is still wanted, never giving up on its own
     * (the player waits for it). Returns false when it stopped because the
     * plan changed and [s] is no longer first.
     */
    private suspend fun fetch(
        s: Song,
        dir: File,
    ): Boolean {
        val file = File(dir, name(s))
        val part = File(dir, name(s) + ".part")
        val e = entries.computeIfAbsent(s.id) { Entry(s.id, part) }
        var backoff = 1_000L
        while (true) {
            if (replan && !stillFirst(s, dir)) return false
            try {
                val finished = withContext(Dispatchers.IO) { download(s, part, e) }
                if (finished) {
                    synchronized(e) {
                        if (!part.renameTo(file)) throw IOException("Could not save ${file.name}")
                        e.file = file
                        e.done = true
                    }
                    e.changed()
                    entries.remove(s.id)
                    measure()
                    return true
                }
                if (e.gone) return true // nothing to fetch; the player moves on
                // The plan changed: carry on from the part if this song still comes first.
            } catch (ex: CancellationException) {
                throw ex
            } catch (ex: IOException) {
                e.failing = true
                e.changed()
                delay(backoff)
                backoff = (backoff * 2).coerceAtMost(5_000)
            }
        }
    }

    /** One request, from where the part ends. True when the file is whole; false when the plan changed or the song is gone. */
    private fun download(
        s: Song,
        part: File,
        e: Entry,
    ): Boolean {
        var have = part.length()
        e.written = have
        val req =
            Request
                .Builder()
                .url(app.api.streamUrl(s.id))
                .apply { if (have > 0) header("Range", "bytes=$have-") }
                .build()
        app.api.songs.newCall(req).execute().use { res ->
            when {
                res.code == 404 || res.code == 410 -> {
                    e.gone = true
                    e.changed()
                    part.delete()
                    return false
                }

                // The part is already whole.
                res.code == 416 && have > 0 && have == s.size -> {
                    return true
                }

                // The part is longer than the file: the file changed. Start again.
                res.code == 416 -> {
                    part.delete()
                    throw IOException("${s.title}: the file changed on the server")
                }

                !res.isSuccessful -> {
                    throw IOException("HTTP ${res.code}")
                }

                // The server ignored the range: start again.
                res.code == 200 && have > 0 -> {
                    have = 0
                    e.written = 0
                }
            }
            val length = res.body.contentLength()
            e.total = if (length >= 0) have + length else s.size.takeIf { it > 0 } ?: -1
            e.failing = false
            RandomAccessFile(part, "rw").use { out ->
                out.setLength(have)
                out.seek(have)
                val input = res.body.byteStream()
                val buf = ByteArray(64 * 1024)
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) break
                    out.write(buf, 0, n)
                    have += n
                    e.written = have
                    e.changed()
                    if (replan) return false
                }
            }
        }
        // Against what the server sent, not the song list, which may be older
        // than the file: else a changed file would be fetched again forever.
        if (e.total > 0 && have != e.total) {
            part.delete()
            throw IOException("${s.title}: got $have of ${e.total} bytes")
        }
        return true
    }

    /** True if [s] is still the first song of the plan not yet whole: the plan changed, but not for it. */
    private suspend fun stillFirst(
        s: Song,
        dir: File,
    ): Boolean {
        replan = false
        val first = plan().firstOrNull { !File(dir, name(it)).exists() }
        if (first?.id == s.id) return true
        replan = true
        return false
    }

    /**
     * Drops the songs played longest ago until the cache is within [limit];
     * never one in [plan]. Also drops any song since downloaded, which the
     * cache never holds.
     */
    private fun trim(
        dir: File,
        limit: Long,
        plan: List<Song>,
    ) {
        val keep = plan.map { name(it) }.toSet()
        val files = dir.listFiles()?.toMutableList() ?: return
        files.removeAll { f ->
            f.name
                .substringBefore('.')
                .toLongOrNull()
                ?.let { app.downloads.file(it) } != null && f.delete()
        }
        var total = files.sumOf { it.length() }
        for (f in files.sortedBy { it.lastModified() }) {
            if (total <= limit) break
            if (f.name.removeSuffix(".part") in keep) continue
            total -= f.length()
            f.delete()
        }
        measure()
    }

    private fun measure() {
        val files = dir()?.listFiles().orEmpty()
        _used.value = files.sumOf { it.length() }
        _songs.value = files.filter { !it.name.endsWith(".part") }.mapNotNull { it.name.substringBefore('.').toLongOrNull() }.toSet()
    }

    /** On the same storage as Downloads, so a promotion is a rename. */
    private fun dir(): File? = app.downloads.dir(DIR)

    private fun name(s: Song): String {
        val ext =
            s.path
                .substringAfterLast('.', "")
                .lowercase()
                .ifEmpty { s.format }
        return "${s.id}.$ext"
    }

    companion object {
        private const val DIR = "cache"

        /** The owner's choice: two songs on mobile data, ten on Wi-Fi (a queue may hold a thousand). */
        private const val AHEAD_MOBILE = 2
        private const val AHEAD_WIFI = 10

        /** A seek further ahead than this past what has arrived reads from the stream instead of waiting. */
        private const val SEEK_AHEAD = 1L shl 20
    }
}
