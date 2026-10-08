package io.github.vivekg7.dhun.data

import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import io.github.vivekg7.dhun.App
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.JsonNull

/**
 * Keeps the working copy and the server in step (docs/plans/006_api_and_sync.md):
 * fetch catalogue changes, push the outbox, pull what changed elsewhere.
 * Runs when the app comes to the foreground, a few seconds after each
 * local change, and when the network returns.
 */
class Sync(
    private val app: App,
) {
    private val mutex = Mutex()
    private val dao get() = app.db.dao()

    private val _error = MutableStateFlow<String?>(null)

    /** The last error, for the menu; null when the last sync worked. */
    val error: StateFlow<String?> = _error

    /** Another device's playback, for hand-off (plan 002). */
    val nowPlaying = MutableStateFlow<NowPlaying?>(null)

    private var pending: Job? = null
    private var retryDelay = RETRY_MIN

    init {
        // Back online: send what piled up while offline.
        app.getSystemService(ConnectivityManager::class.java).registerDefaultNetworkCallback(
            object : ConnectivityManager.NetworkCallback() {
                override fun onAvailable(network: Network) {
                    soon()
                    app.downloads.poke()
                }

                // A hand-over from Wi-Fi to mobile data loses one network as
                // the next arrives: offline only if none has taken its place.
                override fun onLost(network: Network) {
                    app.scope.launch {
                        delay(3_000)
                        if (!hasNetwork()) app.api.reachable.value = false
                    }
                }
            },
        )
    }

    /**
     * Debounced: a burst of taps becomes one request. A failed sync retries
     * with a growing delay while the app or its playback service runs; if the
     * process dies first, the next start sends the outbox, which is on disk.
     * (WorkManager would also sync with the app closed, but brings a second
     * copy of Room into the APK: docs/plans/011_android_app.md.)
     */
    fun soon(delayMs: Long = 2_000) {
        pending?.cancel()
        pending =
            app.scope.launch {
                delay(delayMs)
                if (now()) {
                    retryDelay = RETRY_MIN
                } else {
                    retryDelay = (retryDelay * 2).coerceAtMost(RETRY_MAX)
                    soon(retryDelay)
                }
            }
    }

    /** Returns false when the server could not be reached, so [soon] tries again later. */
    suspend fun now(): Boolean =
        mutex.withLock {
            if (app.prefs.token.isEmpty()) return true
            try {
                pullLibrary()
                // More than 500 ops (a long offline stretch) take several rounds.
                pushedPlaylists = false
                while (pushAndPull() > 0 && dao.outboxSize() > 0) Unit
                // The server rewrote those playlists: fetch them as written.
                if (pushedPlaylists) pullLibrary()
                pullPlays()
                _error.value = null
                true
            } catch (e: ApiException) {
                // A revoked token (password changed, device removed): sign in again.
                if (e.code == 401) app.signOut()
                _error.value = e.message
                e.code !in 500..599
            } catch (e: CancellationException) {
                throw e
            } catch (e: java.io.IOException) {
                _error.value = e.message ?: e.toString()
                false
            } catch (e: Exception) {
                _error.value = e.message ?: e.toString()
                false
            }
        }

    private var pushedPlaylists = false

    /**
     * A playlist with an op still in the outbox is left as edited here, and
     * the cursor stays put so the next pull brings it again once the op has
     * gone (docs/plans/013_playlist_editing.md).
     */
    private suspend fun pullLibrary() {
        val lib = app.api.library(app.prefs.libraryVersion)
        if (lib.songs.isNotEmpty()) dao.putSongs(lib.songs.map { it.toSong() })
        val pending = dao.outbox(Int.MAX_VALUE).map { it.key }

        fun edited(id: Long) = pending.any { it.endsWith(":" + Store.playlistTag(id)) }
        var skipped = false
        val local = dao.localPlaylists()
        for (p in lib.playlists) {
            if (p.deleted) {
                if (edited(p.id)) skipped = true else dao.deletePlaylist(p.id)
                continue
            }
            // One made on this phone, back from the server with its real id. A
            // server older than `ref` is matched by name, once its create has gone.
            val mine =
                local.firstOrNull { it.ref.isNotEmpty() && it.ref == p.ref }
                    ?: local.firstOrNull { p.ref.isEmpty() && !p.shared && it.name == p.name && !edited(it.id) }
            if (mine != null) {
                if (edited(mine.id)) {
                    skipped = true
                    continue
                }
                dao.deletePlaylist(mine.id)
                dao.movePin(Downloads.key(Downloads.PLAYLIST, mine.id.toString()), Downloads.key(Downloads.PLAYLIST, p.id.toString()), p.id.toString())
                replaced[mine.id] = p.id
            } else if (edited(p.id)) {
                skipped = true
                continue
            }
            dao.putPlaylist(Playlist(p.id, p.name, p.path, p.shared, p.songs.joinIds(), p.ref))
        }
        if (!skipped) app.prefs.libraryVersion = lib.version
    }

    /** A playlist made on this phone and the server id it became, for a page still showing the old one. */
    val replaced = java.util.concurrent.ConcurrentHashMap<Long, Long>()

    /** One round trip; returns how many ops the server answered. */
    private suspend fun pushAndPull(): Int {
        val ops = dao.outbox(500)
        if (ops.any { it.json.contains("\"type\":\"playlist.") }) pushedPlaylists = true
        val state = app.api.sync(app.prefs.syncVersion, ops.map { app.api.json.parseToJsonElement(it.json) })
        // applied, duplicate and rejected all leave the outbox: a rejected op
        // can never succeed (api/openapi.yaml).
        val done = state.results.map { it.id }
        // A playlist the server refused to create (a name it reserves) would
        // otherwise stay here forever as a local-only row.
        val refused =
            state.results
                .filter { it.status == "rejected" }
                .map { it.id }
                .toSet()
        for (op in ops) {
            if (op.id in refused && op.json.contains("\"type\":\"playlist.create\"")) {
                op.key
                    .substringAfterLast(":pl", "")
                    .toLongOrNull()
                    ?.let { dao.deletePlaylist(it) }
            }
        }
        if (done.isNotEmpty()) dao.ackOps(done)
        apply(state)
        app.prefs.syncVersion = state.version
        return done.size
    }

    /**
     * Applies what the server says changed. Anything with an op still in
     * the outbox (made while the request was in flight) is left alone: that
     * op goes with the next sync and the server's answer will include it.
     */
    private suspend fun apply(s: SyncState) {
        val pending = dao.outbox(Int.MAX_VALUE).map { it.key }.toSet()
        for (q in s.queues) {
            if (q.deleted) {
                dao.deleteQueue(q.id)
                continue
            }
            if (pending.any { it.endsWith(":${q.id}") }) continue
            dao.putQueue(QueueRow(q.id, q.name, q.songs.joinIds(), q.currentSong, q.positionMs, q.shuffle, q.repeat, parseTime(q.usedAt)))
        }
        for ((kind, items) in listOf(Store.FAV to s.favorites, Store.LATER to s.listenLater)) {
            for (i in items) {
                val at = parseTime(i.at)
                val local = dao.mark(kind, i.song)
                if ("$kind:${i.song}" in pending || (local != null && local.at > at)) continue
                dao.putMark(Mark(kind, i.song, at, i.deleted))
            }
        }
        for (r in s.resume) {
            val at = parseTime(r.at)
            val local = dao.resume(r.song)
            if ("resume:${r.song}" in pending || (local != null && local.at > at)) continue
            dao.putResume(Resume(r.song, r.positionMs, at, r.deleted))
        }
        for ((name, value) in s.settings) {
            if ("setting:$name" in pending) continue
            if (value is JsonNull) dao.deleteSetting(name) else dao.putSetting(Setting(name, value.toString()))
        }
        // Sent only when it changed since the cursor: no news is not "nothing playing".
        s.nowPlaying?.let { nowPlaying.value = it }
    }

    /**
     * On coming to the foreground: what the user's devices played last, for
     * "Continue from …" (docs/plans/017_handoff.md). Asked directly, because
     * after a restart the sync cursor is already past it.
     */
    suspend fun checkHandoff() {
        if (app.prefs.token.isEmpty()) return
        try {
            // Signed in before the app kept its device id.
            if (app.prefs.deviceId == 0L) app.prefs.deviceId = app.api.me().deviceId
            app.api.nowPlaying()?.let { nowPlaying.value = it }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            // Offline, or an answer we cannot read: nothing to hand off from this time.
        }
    }

    private fun hasNetwork(): Boolean {
        val cm = app.getSystemService(ConnectivityManager::class.java)
        return cm.getNetworkCapabilities(cm.activeNetwork)?.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) == true
    }

    private suspend fun pullPlays() {
        val plays = app.api.plays().plays
        dao.clearPlayStats()
        dao.putPlayStats(plays.map { PlayStat(it.song, it.count, parseTime(it.lastPlayedAt)) })
    }
}

// A blip is over in seconds; the 15 minutes this once grew to left the app
// offline long after the network was back (docs/plans/019_networking_and_caching.md).
private const val RETRY_MIN = 5_000L
private const val RETRY_MAX = 2 * 60_000L
