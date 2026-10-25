package io.github.vivekg7.dhun.data

import android.system.Os
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
import java.util.concurrent.TimeUnit
import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock

/**
 * The song cache (docs/plans/019_networking_and_caching.md): original files,
 * fetched whole at the network's full speed, so a song is on the phone
 * seconds after it starts and a later gap in the network does not matter.
 * It keeps the song playing (every song played is kept) and the next two of
 * the queue, or the next ten on Wi-Fi, while playing. The player reads a
 * file while it is still arriving ([open]).
 *
 * A long file (an episode, a book, a mix: [io.github.vivekg7.dhun.play.Playback.isLong])
 * is not fetched whole but as a window around where it plays
 * (docs/plans/037_long_file_cache_window.md): the minutes ahead that
 * `longFiles.cacheAhead` says, ten minutes behind, in a sparse `.window`
 * file the fetcher fills wherever the reader is. A window is never a cached
 * song: it sits outside the music budget, is dropped as soon as its file
 * leaves the plan, and is never promoted to a download.
 *
 * Plain files beside Downloads, not Media3's SimpleCache, so a cached song
 * becomes a download by a rename (docs/plans/007_client_architecture.md);
 * a file's modification time is when it was last played, for dropping the
 * oldest at the limit. Never holds a downloaded song.
 */
class SongCache(
    private val app: App,
) {
    /**
     * A song being fetched, as the player reading it and the fetcher writing
     * it see it. A song's `.part` is written in order from the start; a long
     * file's `.window` is written wherever the reader is, so the entry keeps
     * the runs of bytes that have arrived.
     */
    class Entry(
        val song: Long,
        @Volatile var file: File,
        /** For a window: the bytes fetched ahead of the reader. 0 fetches the whole file, in order. */
        val ahead: Long = 0,
        /** For a window: the bytes kept behind the reader; past twice that, the file is rolled. */
        val behind: Long = 0,
    ) {
        private class Run(
            var start: Long,
            var end: Long,
        )

        private val lock = ReentrantLock()
        private val grew = lock.newCondition()

        /** Sorted, disjoint, never adjacent. Guarded by [lock]. */
        private val have = ArrayList<Run>()

        /** The whole file's length, as the server said, or the song list until it has; else -1. */
        @Volatile var total = -1L

        @Volatile var done = false

        /** The last try failed and the fetcher is waiting to try again: a reader that has caught up says so. */
        @Volatile var failing = false

        /** The server no longer has the song: final, unlike [failing]. */
        @Volatile var gone = false

        /** The fetcher is on this entry; a window needing more bytes pokes it otherwise. */
        @Volatile var fetching = false

        /**
         * Where the reader is, which a window is fetched ahead of and kept
         * behind; -1 until it is known. Set by [track] only once the reader
         * has read on in one piece for [ANCHOR] bytes: a decoder finding its
         * place reads a page here and a page there, and a window at each
         * probe would fetch the whole file in pieces.
         */
        @Volatile var readAt = -1L

        /** Where the reader's last read ended, and where its run of reads in one piece began. */
        @Volatile var lastEnd = -1L
        private var runStart = -1L

        /** Where a reader is blocked waiting for bytes, or -1: a window's fetcher jumps there. */
        @Volatile var waitingAt = -1L

        /** Bumped when [roll] replaces [file]: a reader with the old one open reopens. */
        @Volatile var generation = 0

        fun changed() = lock.withLock { grew.signalAll() }

        /** A read at [pos] is starting: anchors the window once the reader has settled. */
        fun track(pos: Long) {
            if (pos == lastEnd) {
                if (pos - runStart >= ANCHOR) readAt = pos
            } else {
                runStart = pos
            }
        }

        /** Where the bytes run out, reading on from [pos]; [pos] itself when none has arrived there. */
        fun arrived(pos: Long): Long = lock.withLock { end(pos) }

        /** [start] until [end] has been written. */
        fun add(
            start: Long,
            end: Long,
        ) {
            if (end <= start) return
            lock.withLock {
                var s = start
                var e = end
                have.removeAll { r ->
                    val touches = r.start <= e && r.end >= s
                    if (touches) {
                        s = minOf(s, r.start)
                        e = maxOf(e, r.end)
                    }
                    touches
                }
                have.add(Run(s, e))
                have.sortBy { it.start }
                grew.signalAll()
            }
        }

        fun clear() = lock.withLock { have.clear() }

        /** Every byte of the file is here. */
        val whole: Boolean
            get() = lock.withLock { total > 0 && have.size == 1 && have[0].start == 0L && have[0].end == total }

        /**
         * How far a request from [from] goes before stopping: the file's
         * end, the next run already here, and for a window its end when
         * [from] is inside it, else [MIN_FETCH]: a jump to a reader waiting
         * elsewhere fetches enough for that read and no more.
         */
        fun goal(from: Long): Long {
            var g = if (total > 0) total else Long.MAX_VALUE
            if (ahead > 0) {
                val r = readAt
                g = minOf(g, if (r >= 0 && from >= r && from < r + ahead) r + ahead else from + MIN_FETCH)
            }
            lock.withLock { have.firstOrNull { it.start > from }?.let { g = minOf(g, it.start) } }
            return g
        }

        /**
         * Where the fetcher should write next, or null when nothing is wanted
         * now: the file is whole, or the window is full until the reader
         * moves on.
         */
        fun next(): Long? {
            if (whole) return null
            if (ahead == 0L) return arrived(0)
            val w = waitingAt
            if (w >= 0 && arrived(w) == w) return w
            val r = readAt
            if (r < 0) return null
            val a = arrived(r)
            return if ((total <= 0 || a < total) && a < r + ahead) a else null
        }

        /** More than twice [behind] sits before the reader: time to roll. */
        fun rollDue(): Boolean = ahead > 0 && lock.withLock { (have.firstOrNull()?.start ?: readAt) < readAt - 2 * behind }

        /**
         * Copies the bytes from [behind] the reader on into a fresh file that
         * takes this one's place, so a window never holds more than about
         * twice [behind] plus [ahead]. Called by the fetcher between writes,
         * so nothing writes meanwhile; a reader with the old file open sees
         * [generation] change and reopens.
         */
        fun roll() {
            val keepFrom = maxOf(0, readAt - behind)
            val runs = lock.withLock { have.filter { it.end > keepFrom }.map { Run(maxOf(it.start, keepFrom), it.end) } }
            val tmp = File(file.path + ".tmp")
            RandomAccessFile(file, "r").use { src ->
                RandomAccessFile(tmp, "rw").use { dst ->
                    val buf = ByteArray(256 * 1024)
                    for (r in runs) {
                        var p = r.start
                        src.seek(p)
                        dst.seek(p)
                        while (p < r.end) {
                            val n = src.read(buf, 0, minOf(buf.size.toLong(), r.end - p).toInt())
                            if (n < 0) break
                            dst.write(buf, 0, n)
                            p += n
                        }
                    }
                }
            }
            lock.withLock {
                if (!tmp.renameTo(file)) {
                    tmp.delete()
                    return
                }
                have.clear()
                have.addAll(runs)
                generation++
                grew.signalAll()
            }
        }

        /** Blocks until a byte past [pos] has arrived, and returns how many have; or the end, when the file is whole. */
        fun awaitPast(pos: Long): Long {
            lock.withLock {
                try {
                    while (true) {
                        if (gone) throw FileNotFoundException("Song $song is no longer on the server")
                        val e = end(pos)
                        if (e > pos || done) return e
                        // An error lets the player retry and say it is waiting for the network.
                        if (failing) throw IOException("Waiting for the network")
                        waitingAt = pos
                        try {
                            grew.await(250, TimeUnit.MILLISECONDS)
                        } catch (e: InterruptedException) {
                            // The player cancelled the load: a seek, a skip, a release.
                            Thread.currentThread().interrupt()
                            throw InterruptedIOException()
                        }
                    }
                } finally {
                    waitingAt = -1
                }
            }
        }

        private fun end(pos: Long): Long {
            for (r in have) if (pos >= r.start && pos < r.end) return r.end
            return pos
        }

        companion object {
            const val MIN_FETCH = 256L * 1024
            const val ANCHOR = 256L * 1024
        }
    }

    private enum class Outcome { WHOLE, MORE, REPLAN, GONE }

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
            // Which bytes of a window had arrived was only known in memory.
            dir()?.listFiles()?.filter { f -> PARTIAL.drop(1).any { f.name.endsWith(it) } }?.forEach { it.delete() }
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
        return File(dir, s.fileName).takeIf { it.exists() }?.also { it.setLastModified(System.currentTimeMillis()) }
    }

    /**
     * Opens a song not yet whole, to read from [pos] while it arrives; the
     * fetcher is told, as the song the player wants is first in its plan.
     * Null when the cache is off, or, for a song, [pos] is far past what has
     * arrived (a seek ahead), which the stream serves sooner. A window's
     * fetcher jumps to the reader instead. The third is the file's
     * [Entry.generation] as opened.
     */
    fun open(
        song: Long,
        pos: Long,
    ): Triple<Entry, RandomAccessFile, Int>? {
        if (app.prefs.cacheLimitGb == 0) return null
        val s = app.catalog.value.byId[song] ?: return null
        val dir = dir() ?: return null
        val e =
            entries.computeIfAbsent(song) {
                poke()
                entry(s, dir)
            }
        synchronized(e) {
            if (e.done || e.gone) return null
            if (e.ahead == 0L && pos > e.arrived(0) + SEEK_AHEAD) return null
            // Finished just now, between the player's look for a whole file and this.
            if (File(dir, s.fileName).exists()) return null.also { entries.remove(song, e) }
            e.file.createNewFile()
            // Opened under the lock that renames it: the descriptor stays good
            // after the rename. The generation is read before the file is
            // opened, so a roll between the two makes the reader reopen at
            // worst, never read a file the fetcher has left.
            val generation = e.generation
            return Triple(e, RandomAccessFile(e.file, "r"), generation)
        }
    }

    /**
     * The reader is about to read at [pos] of [e]. Unless the fetcher is on
     * this entry and will see for itself, it is sent back when the reader is
     * about to wait, or a window has run down to its last megabyte ahead.
     */
    fun reading(
        e: Entry,
        pos: Long,
    ) {
        e.track(pos)
        if (e.ahead == 0L || e.fetching) return
        val r = e.readAt
        if (e.arrived(pos) == pos || (r >= 0 && e.arrived(r) < minOf(e.total, r + e.ahead) - SEEK_AHEAD)) poke()
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
            if (File(dir, s.fileName).exists()) continue
            if (!fetch(s, dir, current = s.id == plan.first().id)) return
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
                .filter { !it.onPhone && app.downloads.file(it.id) == null }
        }

    /**
     * The entry for [s]: a window for a long file whose size and length are
     * known, else a part written in order, which resumes from what is there.
     */
    private fun entry(
        s: Song,
        dir: File,
    ): Entry {
        val ahead = aheadBytes(s)
        val file = File(dir, s.fileName + if (ahead > 0) WINDOW else PART)
        return Entry(s.id, file, ahead, if (ahead > 0) BEHIND_MINUTES * perMinute(s) else 0).also {
            if (ahead == 0L) it.add(0, file.length())
            // The song list's size until the server says: opened without
            // a length, an Ogg file stays unseekable for as long as it is
            // loaded, and an MP3 without a Xing header has no duration.
            if (s.size > 0) it.total = s.size
        }
    }

    /** The bytes a minute of [s] takes, or 0 when its size or length is unknown. */
    private fun perMinute(s: Song) = if (s.size > 0 && s.durationMs > 0) s.size * 60_000 / s.durationMs else 0

    /** How far ahead of the reader [s] is cached: 0 for a song, or a long file cached whole. */
    private fun aheadBytes(s: Song): Long {
        val perMinute = perMinute(s)
        if (perMinute == 0L || !app.playback.isLong(s)) return 0
        return Synced.longCacheAhead.of(app.store.settings.value) * perMinute
    }

    /**
     * Fetches [s] into the cache: a song whole, a long file's window up to
     * date. Retries a failure while the song is still wanted, never giving
     * up on its own (the player waits for it). Returns false when it stopped
     * because the plan changed and [s] is no longer first.
     */
    private suspend fun fetch(
        s: Song,
        dir: File,
        current: Boolean,
    ): Boolean {
        val e = entries.computeIfAbsent(s.id) { entry(s, dir) }
        // The next file's window starts at its start; the one playing anchors on its reader.
        if (!current && e.readAt < 0) e.readAt = 0
        e.fetching = true
        try {
            var backoff = 1_000L
            while (true) {
                if (replan && !stillFirst(s, dir)) return false
                val from =
                    e.next() ?: run {
                        if (e.whole) finish(s, e, dir)
                        return true
                    }
                try {
                    when (withContext(Dispatchers.IO) { download(s, e, from) }) {
                        Outcome.WHOLE -> {
                            finish(s, e, dir)
                            return true
                        }

                        // Nothing to fetch; the player moves on.
                        Outcome.GONE -> {
                            return true
                        }

                        // next() says where, and a changed plan whether this song still comes first.
                        Outcome.MORE, Outcome.REPLAN -> {}
                    }
                } catch (ex: CancellationException) {
                    throw ex
                } catch (ex: IOException) {
                    e.failing = true
                    e.changed()
                    delay(backoff)
                    backoff = (backoff * 2).coerceAtMost(5_000)
                }
            }
        } finally {
            e.fetching = false
        }
    }

    /** The file is whole: a cached song now, by its final name. */
    private fun finish(
        s: Song,
        e: Entry,
        dir: File,
    ) {
        val file = File(dir, s.fileName)
        synchronized(e) {
            if (!e.file.renameTo(file)) throw IOException("Could not save ${file.name}")
            e.file = file
            e.done = true
        }
        e.changed()
        entries.remove(s.id)
        measure()
    }

    /**
     * One request, from [from], written until the file is whole, the goal is
     * reached, a reader waits elsewhere, or the plan changed.
     */
    private fun download(
        s: Song,
        e: Entry,
        from: Long,
    ): Outcome {
        var have = from
        // A song is asked for to its end; a window only as far as its goal.
        val end = e.goal(from)
        val range = if (e.ahead > 0 && end < Long.MAX_VALUE) "bytes=$have-${end - 1}" else "bytes=$have-"
        val req =
            Request
                .Builder()
                .url(app.api.streamUrl(s.id))
                .apply { if (have > 0 || e.ahead > 0) header("Range", range) }
                .build()
        app.api.songs.newCall(req).execute().use { res ->
            when {
                res.code == 404 || res.code == 410 -> {
                    e.gone = true
                    e.changed()
                    e.file.delete()
                    return Outcome.GONE
                }

                // The part is already whole.
                res.code == 416 && have > 0 && have == s.size && e.whole -> {
                    return Outcome.WHOLE
                }

                // The part is longer than the file: the file changed. Start again.
                res.code == 416 -> {
                    e.file.delete()
                    e.clear()
                    throw IOException("${s.title}: the file changed on the server")
                }

                !res.isSuccessful -> {
                    throw IOException("HTTP ${res.code}")
                }

                // The server ignored the range: start again.
                res.code == 200 && have > 0 -> {
                    have = 0
                    e.clear()
                }
            }
            // The file's length: after the slash of a ranged answer's Content-Range,
            // else what is left to come; the song list's size until the server says.
            val length = res.body.contentLength()
            val whole = res.header("Content-Range")?.substringAfter('/', "")?.toLongOrNull()
            e.total = whole ?: if (length >= 0 && (e.ahead == 0L || res.code == 200)) have + length else s.size.takeIf { it > 0 } ?: -1
            e.failing = false
            var out = RandomAccessFile(e.file, "rw")
            try {
                if (e.ahead == 0L) out.setLength(have)
                out.seek(have)
                val input = res.body.byteStream()
                val buf = ByteArray(64 * 1024)
                while (true) {
                    if (have >= end) return Outcome.MORE
                    val n = input.read(buf, 0, minOf(buf.size.toLong(), end - have).toInt())
                    if (n < 0) break
                    out.write(buf, 0, n)
                    e.add(have, have + n)
                    have += n
                    if (replan) return Outcome.REPLAN
                    if (e.ahead > 0) {
                        // A reader waiting behind in a hole, or far ahead: go there.
                        val w = e.waitingAt
                        if (w >= 0 && ((w < have && e.arrived(w) == w) || w > have + SEEK_AHEAD)) return Outcome.MORE
                        if (e.rollDue()) {
                            out.close()
                            e.roll()
                            out = RandomAccessFile(e.file, "rw")
                            out.seek(have)
                        }
                    }
                }
            } finally {
                out.close()
            }
        }
        if (e.total <= 0) e.total = have
        if (e.whole) return Outcome.WHOLE
        // Against what the server sent, not the song list, which may be older
        // than the file: else a changed file would be fetched again forever.
        if (e.ahead == 0L && have != e.total) {
            e.file.delete()
            e.clear()
            throw IOException("${s.title}: got $have of ${e.total} bytes")
        }
        return Outcome.MORE
    }

    /** True if [s] is still the first song of the plan not yet whole: the plan changed, but not for it. */
    private suspend fun stillFirst(
        s: Song,
        dir: File,
    ): Boolean {
        replan = false
        val first = plan().firstOrNull { !File(dir, it.fileName).exists() }
        if (first?.id == s.id) return true
        replan = true
        return false
    }

    /**
     * Drops the songs played longest ago until the cache is within [limit];
     * never one in [plan]. Also drops any song since downloaded, which the
     * cache never holds, and, once it has left the plan, any long file that
     * is cached as a window: its window, or the whole file an earlier
     * version or setting kept. A window is only ever for the file playing,
     * and sits outside the music budget.
     */
    private fun trim(
        dir: File,
        limit: Long,
        plan: List<Song>,
    ) {
        val keep = plan.map { it.fileName }.toSet()
        val files = dir.listFiles()?.toMutableList() ?: return
        files.removeAll { f ->
            val id = f.name.substringBefore('.').toLongOrNull()
            val downloaded = id?.let { app.downloads.file(it) } != null
            val windowed = f.name.endsWith(WINDOW) || id?.let { app.catalog.value.byId[it] }?.let { aheadBytes(it) > 0 } == true
            (downloaded || (windowed && base(f.name) !in keep)) && drop(f, id)
        }
        var total = files.filter { !it.name.endsWith(WINDOW) }.sumOf { it.length() }
        for (f in files.sortedBy { it.lastModified() }) {
            if (total <= limit) break
            if (f.name.endsWith(WINDOW) || base(f.name) in keep) continue
            total -= f.length()
            drop(f, f.name.substringBefore('.').toLongOrNull())
        }
        measure()
    }

    /** Deletes [f], and forgets what an entry thought was in it. */
    private fun drop(
        f: File,
        id: Long?,
    ): Boolean {
        if (!f.delete()) return false
        if (id != null) entries.remove(id)
        return true
    }

    private fun measure() {
        val files = dir()?.listFiles().orEmpty()
        // Blocks, not length: a window is a sparse file as long as the whole book.
        _used.value = files.sumOf { f -> runCatching { Os.stat(f.path).st_blocks * 512L }.getOrDefault(f.length()) }
        _songs.value =
            files
                .filter { f -> PARTIAL.none { f.name.endsWith(it) } }
                .mapNotNull { it.name.substringBefore('.').toLongOrNull() }
                .toSet()
    }

    /** On the same storage as Downloads, so a promotion is a rename. */
    private fun dir(): File? = app.downloads.dir(DIR)

    /** A cached file's name without a `.part` or `.window`. */
    private fun base(name: String) = name.removeSuffix(PART).removeSuffix(WINDOW)

    companion object {
        private const val DIR = "cache"
        private const val PART = ".part"
        private const val WINDOW = ".window"

        /** Not a cached song: a part, a window, or a window's roll in progress. */
        private val PARTIAL = listOf(PART, WINDOW, ".tmp")

        /** The owner's choice: two songs on mobile data, ten on Wi-Fi (a queue may hold a thousand). */
        private const val AHEAD_MOBILE = 2
        private const val AHEAD_WIFI = 10

        /** How much of a long file is kept behind the reader (docs/plans/037_long_file_cache_window.md). */
        private const val BEHIND_MINUTES = 10

        /** A seek further ahead than this past what has arrived reads from the stream instead of waiting. */
        private const val SEEK_AHEAD = 1L shl 20
    }
}
