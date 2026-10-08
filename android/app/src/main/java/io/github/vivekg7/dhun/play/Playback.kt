package io.github.vivekg7.dhun.play

import android.net.Uri
import android.os.SystemClock
import androidx.annotation.OptIn
import androidx.core.net.toUri
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.datasource.HttpDataSource
import androidx.media3.datasource.ResolvingDataSource
import androidx.media3.datasource.okhttp.OkHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.analytics.AnalyticsListener
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.exoplayer.source.LoadEventInfo
import androidx.media3.exoplayer.source.MediaLoadData
import androidx.media3.exoplayer.upstream.DefaultLoadErrorHandlingPolicy
import androidx.media3.exoplayer.upstream.LoadErrorHandlingPolicy
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Api
import io.github.vivekg7.dhun.data.Listen
import io.github.vivekg7.dhun.data.NowPlaying
import io.github.vivekg7.dhun.data.QueueRow
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.int
import io.github.vivekg7.dhun.data.joinIds
import io.github.vivekg7.dhun.data.parseTime
import io.github.vivekg7.dhun.data.songIds
import io.github.vivekg7.dhun.data.string
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonNull
import java.io.IOException
import java.util.TimeZone
import java.util.UUID
import java.util.concurrent.TimeUnit

/**
 * The player and the queues around it (docs/plans/007_client_architecture.md).
 * Media3's player holds one list, so only the active queue is loaded into
 * it; switching saves the outgoing queue's song and position and loads the
 * incoming one where it was left. Every listen is logged (plan 008) and long
 * files keep a resume point (plan 009).
 *
 * Lives as long as the process, on the main thread like the player itself.
 * [PlaybackService] only wraps it in a media session.
 *
 * Opts in to Media3's "unstable" APIs (they may change between versions),
 * which cover much of ExoPlayer and the session.
 */
@OptIn(UnstableApi::class)
class Playback(
    private val app: App,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val store get() = app.store

    val player: ExoPlayer =
        ExoPlayer
            .Builder(app)
            .setMediaSourceFactory(DefaultMediaSourceFactory(dataSource()).setLoadErrorHandlingPolicy(WaitForNetwork))
            // Pause for calls and other apps, and when headphones are unplugged.
            .setAudioAttributes(
                AudioAttributes
                    .Builder()
                    .setUsage(C.USAGE_MEDIA)
                    .setContentType(C.AUDIO_CONTENT_TYPE_MUSIC)
                    .build(),
                true,
            ).setHandleAudioBecomingNoisy(true)
            .setWakeMode(C.WAKE_MODE_NETWORK)
            .build()

    /**
     * Items keep the stream URL; when a song is opened, a downloaded file
     * takes its place (docs/plans/012_downloads.md). Resolving at open time
     * means a download that finishes while the queue is loaded is used too.
     */
    private fun dataSource(): ResolvingDataSource.Factory {
        // A stalled stream is retried after 10 s, not OkHttp's 30: the next try may well work.
        val http =
            app.api.http
                .newBuilder()
                .readTimeout(10, TimeUnit.SECONDS)
                .build()
        val stream = OkHttpDataSource.Factory(http).setCacheControl(Api.NO_STORE)
        return ResolvingDataSource.Factory(DefaultDataSource.Factory(app, stream)) { spec ->
            val id = spec.uri.lastPathSegment?.toLongOrNull()
            val file = if (id != null && spec.uri.toString() == app.api.streamUrl(id)) app.downloads.file(id) else null
            if (file != null) spec.withUri(Uri.fromFile(file)) else spec
        }
    }

    val queues: StateFlow<List<QueueRow>> =
        app.db
            .dao()
            .queues()
            .stateIn(scope, SharingStarted.Eagerly, emptyList())
    val activeId = MutableStateFlow(app.prefs.activeQueue)
    val active: StateFlow<QueueRow?> =
        combine(queues, activeId) { qs, id -> qs.firstOrNull { it.id == id } }
            .stateIn(scope, SharingStarted.Eagerly, null)

    /** The current song, or null with nothing loaded. */
    val current = MutableStateFlow<Song?>(null)
    val playing = MutableStateFlow(false)

    /**
     * Held up because the song cannot be fetched: the player keeps trying
     * (docs/plans/019_networking_and_caching.md), and Now playing says so.
     */
    val waiting = MutableStateFlow(false)

    /** A load failed since the player last had what it needed. */
    private var loadFailed = false

    /** Songs in a row that could not be played at all, so a queue of them is not tried forever. */
    private var failures = 0

    /** The sleep timer (docs/plans/014_sleep_timer.md). */
    val sleep = SleepTimer(player, scope)

    /** A long file with a resume point, waiting for the user's answer ("ask", plan 009). */
    val offerResume = MutableStateFlow<Pair<Song, Long>?>(null)

    private val settings = app.store.settings.stateIn(scope, SharingStarted.Eagerly, emptyMap())

    /** Speed and pitch (docs/plans/016_speed_and_pitch.md): this device's everyday one. */
    val everyday = MutableStateFlow(app.prefs.tempo)

    /** The playing song's own speed and pitch, synced; null when it follows [everyday]. */
    val songTempo: StateFlow<Tempo?> =
        combine(current, settings) { s, m -> s?.let { Tempo.of(m[Tempo.songSetting(it.id)]) } }
            .stateIn(scope, SharingStarted.Eagerly, null)

    /** What the player uses now. */
    val tempo: StateFlow<Tempo> = combine(everyday, songTempo) { e, s -> s ?: e }.stateIn(scope, SharingStarted.Eagerly, Tempo.NORMAL)

    /** When this device last reported its playback, and the hand-off last dismissed (docs/plans/017_handoff.md). */
    private val stateAt = MutableStateFlow(app.prefs.stateAt)
    private val dismissed = MutableStateFlow(app.prefs.handoffDismissed)

    /**
     * "Continue from MacBook": another device's playback, offered while this
     * one is not playing, when it is newer than anything this device played
     * and its queue and song are here. Null when there is nothing to offer.
     */
    val handoff: StateFlow<NowPlaying?> =
        combine(app.sync.nowPlaying, playing, stateAt, dismissed, combine(queues, app.catalog, ::Pair)) { np, playing, mine, gone, (qs, catalog) ->
            np?.takeIf {
                !playing &&
                    app.prefs.deviceId != 0L &&
                    it.deviceId != app.prefs.deviceId &&
                    parseTime(it.at) > mine &&
                    it.at != gone &&
                    qs.any { q -> q.id == it.queue } &&
                    catalog.byId[it.song] != null
            }
        }.stateIn(scope, SharingStarted.Eagerly, null)

    /** The speed the open listen's heard time is counted at, until the next change. */
    private var heardSpeed = 1f

    private var open: Open? = null

    init {
        player.addListener(Events())
        player.addAnalyticsListener(
            object : AnalyticsListener {
                override fun onLoadError(
                    eventTime: AnalyticsListener.EventTime,
                    loadEventInfo: LoadEventInfo,
                    mediaLoadData: MediaLoadData,
                    error: IOException,
                    wasCanceled: Boolean,
                ) {
                    loadFailed = true
                    updateWaiting()
                }
            },
        )
        scope.launch {
            tempo.collect {
                // Time heard so far counts at the old speed.
                open?.let(::count)
                heardSpeed = it.speed
                player.playbackParameters = it.params
            }
        }
        scope.launch {
            closeInterrupted()
            // Reload the queue that was playing when the app last stopped,
            // paused, once the catalogue it refers to is in memory.
            app.catalog.first { it.songs.isNotEmpty() }
            val q =
                app.db
                    .dao()
                    .queues()
                    .first()
                    .firstOrNull { it.id == app.prefs.activeQueue }
            if (q != null && player.mediaItemCount == 0) load(q, play = false)
        }
        scope.launch {
            while (true) {
                delay(10_000)
                if (player.isPlaying) tick()
            }
        }
    }

    /**
     * Sets speed and pitch: for the playing song only (synced, as its own
     * setting), or as this device's everyday one.
     */
    fun setTempo(
        t: Tempo,
        onlyThisSong: Boolean,
    ) {
        val s = current.value
        if (onlyThisSong && s != null) {
            scope.launch { store.setting(Tempo.songSetting(s.id), t.json()) }
        } else {
            app.prefs.tempo = t
            everyday.value = t
        }
    }

    /** The playing song follows the everyday speed and pitch again. */
    fun clearSongTempo() {
        val s = current.value ?: return
        scope.launch { store.setting(Tempo.songSetting(s.id), JsonNull) }
    }

    /**
     * Where a hand-off continues: the other device's place, moved on by the
     * time passed if it was still playing when it last reported, and never
     * at the very end of the song.
     */
    fun placeOf(np: NowPlaying): Long {
        var pos = np.positionMs
        if (np.playing) pos += (System.currentTimeMillis() - parseTime(np.at)).coerceAtLeast(0)
        val length =
            app.catalog.value.byId[np.song]
                ?.durationMs ?: 0
        if (length > 0) pos = pos.coerceAtMost(length - END_MARGIN_MS)
        return pos.coerceAtLeast(0)
    }

    /** Takes over from another device: its queue (queues are synced), its song, and its place. */
    fun continueFrom(np: NowPlaying) =
        scope.launch {
            val q = queues.value.firstOrNull { it.id == np.queue } ?: return@launch
            val song = app.catalog.value.byId[np.song] ?: return@launch
            val pos = placeOf(np)
            if (q.id != activeId.value) saveActive()
            load(q.copy(currentSong = song.id, positionMs = pos, usedAt = System.currentTimeMillis()), play = true, position = pos)
        }

    /** ✕ on the offer: not offered again, though a newer playback on that device will be. */
    fun dismissHandoff(np: NowPlaying) {
        app.prefs.handoffDismissed = np.at
        dismissed.value = np.at
    }

    /** Tells the server what this device plays, for the others' hand-off; a newer report hides this device's own offer. */
    private fun report(
        q: QueueRow,
        s: Song,
        pos: Long,
        isPlaying: Boolean,
    ) {
        val now = System.currentTimeMillis()
        app.prefs.stateAt = now
        stateAt.value = now
        scope.launch { store.playbackState(q.id, s.id, pos, isPlaying) }
    }

    /** On sign-out: the open listen belongs to the account being left, and is dropped. */
    fun reset() {
        sleep.cancel()
        open = null
        // The hand-off belongs to the account being left too.
        app.sync.nowPlaying.value = null
        stateAt.value = 0
        dismissed.value = ""
        player.clearMediaItems()
        current.value = null
        offerResume.value = null
        setActive("")
    }

    // --- Queues -----------------------------------------------------------

    /**
     * Plays [songs] from [start] in a new queue named [name] (AGENTS.md:
     * playing from a list never overwrites the current queue). A queue
     * already called [name] is reused and refilled rather than duplicated,
     * so tapping songs in one album does not leave "Album (2)", "Album (3)".
     */
    fun play(
        name: String,
        source: String,
        songs: List<Song>,
        start: Int,
    ) = scope.launch {
        if (songs.isEmpty()) return@launch
        saveActive()
        val first = songs[start.coerceIn(songs.indices)]
        val ids = songs.map { it.id }.distinct().joinIds()
        val now = System.currentTimeMillis()
        val same = queues.value.firstOrNull { it.name.equals(name, ignoreCase = true) }
        val q =
            if (same != null) {
                same.copy(songs = ids, currentSong = first.id, positionMs = 0, usedAt = now).also {
                    store.replaceQueue(it)
                    store.setCurrent(it)
                }
            } else {
                // At most 20 queues: the least recently used one goes, as on the server.
                queues.value.drop(MAX_QUEUES - 1).forEach { store.deleteQueue(it.id) }
                QueueRow(UUID.randomUUID().toString(), name, ids, first.id, 0, false, "off", now).also { store.createQueue(it) }
            }
        app.prefs.setQueueSource(q.id, source)
        load(q, play = true)
    }

    fun switchTo(id: String) =
        scope.launch {
            if (id == activeId.value) return@launch
            val q = queues.value.firstOrNull { it.id == id } ?: return@launch
            saveActive()
            load(q.copy(usedAt = System.currentTimeMillis()), play = true)
        }

    /** Plays [song] of the active queue. */
    fun playAt(index: Int) {
        player.seekTo(index, 0)
        player.play()
    }

    /** After the current song; songs already queued are moved, never duplicated. */
    fun playNext(songs: List<Song>) = insert(songs, next = true)

    fun addToQueue(songs: List<Song>) = insert(songs, next = false)

    /**
     * The playing queue's order, as the player has it. Edits go through the
     * player and are read back from it, so a song missing from the
     * catalogue (and so never loaded) cannot shift the positions.
     */
    private fun playerIds() = (0 until player.mediaItemCount).map { player.getMediaItemAt(it).mediaId.toLong() }

    private fun insert(
        songs: List<Song>,
        next: Boolean,
    ) = scope.launch {
        val q = active.value
        if (q == null) {
            play("Queue", "", songs, 0)
            return@launch
        }
        val playing = current.value?.id
        val adding =
            songs
                .filter {
                    it.id != playing &&
                        app.catalog.value.byId
                            .containsKey(it.id)
                }.distinctBy { it.id }
        if (adding.isEmpty()) return@launch
        for (s in adding) {
            val i = playerIds().indexOf(s.id)
            if (i >= 0) player.removeMediaItem(i)
        }
        val at = if (next) player.currentMediaItemIndex + 1 else player.mediaItemCount
        player.addMediaItems(at, adding.map(::item))
        val ids = playerIds()
        store.insertIntoQueue(q.copy(songs = ids.joinIds()), adding.map { it.id }, if (at == 0) 0 else ids[at - 1])
    }

    fun removeAt(index: Int) =
        scope.launch {
            val q = active.value ?: return@launch
            if (index !in 0 until player.mediaItemCount) return@launch
            val id = player.getMediaItemAt(index).mediaId.toLong()
            player.removeMediaItem(index)
            store.removeFromQueue(q.copy(songs = playerIds().joinIds()), listOf(id))
        }

    fun move(
        from: Int,
        to: Int,
    ) = scope.launch {
        val q = active.value ?: return@launch
        val n = player.mediaItemCount
        if (from !in 0 until n || to !in 0 until n || from == to) return@launch
        val id = player.getMediaItemAt(from).mediaId.toLong()
        player.moveMediaItem(from, to)
        val ids = playerIds()
        store.moveInQueue(q.copy(songs = ids.joinIds()), id, if (to == 0) 0 else ids[to - 1])
    }

    fun rename(
        id: String,
        name: String,
    ) = scope.launch {
        val q = queues.value.firstOrNull { it.id == id } ?: return@launch
        store.renameQueue(q.copy(name = name))
    }

    fun delete(id: String) =
        scope.launch {
            if (id == activeId.value) {
                close("stopped")
                player.clearMediaItems()
                current.value = null
                setActive("")
                store.deleteQueue(id)
                queues.value.firstOrNull { it.id != id }?.let { load(it, play = false) }
            } else {
                store.deleteQueue(id)
            }
        }

    fun toggleShuffle() =
        scope.launch {
            val q = active.value ?: return@launch
            player.shuffleModeEnabled = !q.shuffle
            store.setMode(q.copy(shuffle = !q.shuffle))
        }

    /** off → whole queue → this song → off. */
    fun cycleRepeat() =
        scope.launch {
            val q = active.value ?: return@launch
            val next =
                when (q.repeat) {
                    "off" -> "queue"
                    "queue" -> "song"
                    else -> "off"
                }
            player.repeatMode = repeatMode(next)
            store.setMode(q.copy(repeat = next))
        }

    fun answerResume(accept: Boolean) {
        val (song, pos) = offerResume.value ?: return
        offerResume.value = null
        if (accept && current.value?.id == song.id) player.seekTo(pos)
    }

    private suspend fun load(
        q: QueueRow,
        play: Boolean,
        position: Long? = null,
    ) {
        close("switched")
        val songs = app.catalog.value.songsOf(songIds(q.songs))
        setActive(q.id)
        if (songs.isEmpty()) {
            player.clearMediaItems()
            current.value = null
            return
        }
        val index = songs.indexOfFirst { it.id == q.currentSong }.coerceAtLeast(0)
        val song = songs[index]
        // A hand-off names its place; else, for a long file, the resume
        // point is kept up to date across devices and wins over the queue's
        // own position (plan 009).
        val start = position?.also { offerResume.value = null } ?: startPosition(song) ?: if (song.id == q.currentSong) q.positionMs else 0
        player.setMediaItems(songs.map(::item), index, start)
        player.shuffleModeEnabled = q.shuffle
        player.repeatMode = repeatMode(q.repeat)
        player.prepare()
        if (play) player.play()
        current.value = song
        store.putQueue(q)
        openListen(song, start)
    }

    private fun setActive(id: String) {
        activeId.value = id
        app.prefs.activeQueue = id
    }

    /** Records where the outgoing queue was, before another is loaded. */
    private suspend fun saveActive() {
        val q = active.value ?: return
        val song = current.value ?: return
        val pos = player.currentPosition
        store.setCurrent(q.copy(currentSong = song.id, positionMs = pos))
        saveResume(song, pos)
    }

    private fun item(s: Song) =
        MediaItem
            .Builder()
            .setMediaId(s.id.toString())
            .setUri(app.api.streamUrl(s.id))
            .setMediaMetadata(
                MediaMetadata
                    .Builder()
                    .setTitle(s.title)
                    .setArtist(s.displayArtist)
                    .setAlbumTitle(s.album)
                    .apply { if (s.hasArt) setArtworkUri(app.api.artUrl(s.id, 512).toUri()) }
                    .build(),
            ).build()

    // --- Long files (plan 009) ---------------------------------------------

    private fun isLong(s: Song) = s.durationMs >= settings.value.int("longFiles.minMinutes", 15) * 60_000L

    /** Where to start [s]: its resume point under "auto"; under "ask", offered instead. */
    private suspend fun startPosition(s: Song): Long? {
        offerResume.value = null
        if (!isLong(s)) return null
        val r =
            app.db
                .dao()
                .resume(s.id)
                ?.takeIf { !it.deleted && it.positionMs > 0 } ?: return null
        return when (settings.value.string("longFiles.resume", "auto")) {
            "auto" -> {
                r.positionMs
            }

            "ask" -> {
                offerResume.value = s to r.positionMs
                null
            }

            else -> {
                null
            }
        }
    }

    private suspend fun saveResume(
        s: Song,
        pos: Long,
    ) {
        if (isLong(s) && pos > 0) store.resume(s.id, pos)
    }

    // --- Listens (plan 008) -------------------------------------------------

    @Serializable
    private data class Open(
        val song: Long,
        /** When it began to play: a song loaded but never started is not a listen yet. */
        var startedAt: Long,
        val fromMs: Long,
        val queue: String,
        val source: String,
        val shuffle: Boolean,
        var heardMs: Long = 0,
        var lastPos: Long = 0,
        var lastAt: Long = 0,
    ) {
        @kotlinx.serialization.Transient var since: Long = 0
    }

    private fun openListen(
        s: Song,
        from: Long,
    ) {
        val q = active.value
        open =
            Open(s.id, 0, from, q?.id ?: "", q?.let { app.prefs.queueSource(it.id) } ?: "", player.shuffleModeEnabled, lastPos = from)
                .also { if (player.isPlaying) started(it) }
    }

    private fun started(o: Open) {
        o.since = SystemClock.elapsedRealtime()
        if (o.startedAt == 0L) o.startedAt = System.currentTimeMillis()
    }

    private suspend fun close(
        end: String,
        toMs: Long = player.currentPosition,
    ) {
        val o = open ?: return
        open = null
        app.prefs.openListen = ""
        count(o)
        // A song that never actually played was not a listen.
        if (o.heardMs <= 0) return
        store.play(listen(o, end, toMs, System.currentTimeMillis()))
    }

    private fun listen(
        o: Open,
        end: String,
        toMs: Long,
        endedAt: Long,
    ) = Listen(
        o.song,
        o.startedAt,
        endedAt,
        o.heardMs,
        o.fromMs,
        toMs,
        end,
        o.source,
        o.queue,
        o.shuffle,
        TimeZone.getDefault().getOffset(o.startedAt) / 60_000,
    )

    /**
     * Adds the time played since the last count to the open listen, as song
     * time: two minutes at 1.5× heard three minutes of the song, which is
     * what the play count's "half the song heard" compares (plan 008).
     */
    private fun count(o: Open) {
        if (o.since <= 0) return
        val now = SystemClock.elapsedRealtime()
        o.heardMs += ((now - o.since) * heardSpeed).toLong()
        o.since = now
    }

    /** A listen still open from a process that died is closed as interrupted. */
    private suspend fun closeInterrupted() {
        val text = app.prefs.openListen.ifEmpty { return }
        app.prefs.openListen = ""
        val o = runCatching { app.api.json.decodeFromString<Open>(text) }.getOrNull() ?: return
        if (o.heardMs > 0) store.play(listen(o, "interrupted", o.lastPos, o.lastAt))
    }

    /** Every 10 s of playing: save the open listen, and every 30 s the positions. */
    private var ticks = 0

    private suspend fun tick() {
        val o = open ?: return
        count(o)
        o.lastPos = player.currentPosition
        o.lastAt = System.currentTimeMillis()
        app.prefs.openListen = app.api.json.encodeToString(Open.serializer(), o)
        if (++ticks % 3 == 0) {
            val q = active.value ?: return
            val s = current.value ?: return
            store.setCurrent(q.copy(currentSong = s.id, positionMs = o.lastPos))
            saveResume(s, o.lastPos)
            report(q, s, o.lastPos, true)
        }
    }

    private inner class Events : Player.Listener {
        override fun onIsPlayingChanged(isPlaying: Boolean) {
            playing.value = isPlaying
            if (isPlaying) failures = 0
            val o = open
            if (o != null) {
                if (isPlaying) {
                    started(o)
                } else if (o.since > 0) {
                    count(o)
                    o.since = 0
                }
            }
            val q = active.value ?: return
            val s = current.value ?: return
            val pos = player.currentPosition
            report(q, s, pos, isPlaying)
            scope.launch {
                if (!isPlaying) {
                    store.setCurrent(q.copy(currentSong = s.id, positionMs = pos))
                    saveResume(s, pos)
                }
            }
        }

        /** Fires before the new song is current, with where the old one ended. */
        override fun onPositionDiscontinuity(
            old: Player.PositionInfo,
            new: Player.PositionInfo,
            reason: Int,
        ) {
            val oldId = old.mediaItem?.mediaId ?: return
            val sameSong = oldId == new.mediaItem?.mediaId
            val end =
                when {
                    reason == Player.DISCONTINUITY_REASON_AUTO_TRANSITION -> {
                        "finished"
                    }

                    sameSong -> {
                        // A seek within the song: the listen goes on.
                        return
                    }

                    reason == Player.DISCONTINUITY_REASON_SEEK -> {
                        val t = player.currentTimeline
                        when (new.mediaItemIndex) {
                            t.getNextWindowIndex(old.mediaItemIndex, Player.REPEAT_MODE_OFF, player.shuffleModeEnabled) -> "skipped"
                            t.getPreviousWindowIndex(old.mediaItemIndex, Player.REPEAT_MODE_OFF, player.shuffleModeEnabled) -> "previous"
                            else -> "switched"
                        }
                    }

                    else -> {
                        "skipped"
                    }
                }
            val finishedSong = current.value
            scope.launch {
                close(end, toMs = if (end == "finished") finishedSong?.durationMs ?: old.positionMs else old.positionMs)
                // Heard to the end: a long file's resume point is done with.
                if (end == "finished" && finishedSong != null && isLong(finishedSong)) {
                    store.resume(finishedSong.id, null)
                } else if (finishedSong != null) {
                    saveResume(finishedSong, old.positionMs)
                }
            }
        }

        override fun onMediaItemTransition(
            item: MediaItem?,
            reason: Int,
        ) {
            // Loading a queue is handled in load(); this is a change within it.
            if (reason == Player.MEDIA_ITEM_TRANSITION_REASON_PLAYLIST_CHANGED) return
            sleep.onNextSong()
            val song = item?.mediaId?.toLongOrNull()?.let { app.catalog.value.byId[it] } ?: return
            current.value = song
            scope.launch {
                val start = startPosition(song)
                if (start != null) player.seekTo(start)
                openListen(song, start ?: 0)
                val q = active.value ?: return@launch
                store.setCurrent(q.copy(currentSong = song.id, positionMs = start ?: 0))
                // The next song, for a hand-off from another device.
                if (player.isPlaying) report(q, song, start ?: 0, true)
            }
        }

        /**
         * A network error never gets here: [WaitForNetwork] tries again until
         * it works. What does is final (the server no longer has the song, a
         * download's file is gone), so the player goes on to the next song.
         */
        override fun onPlayerError(error: PlaybackException) {
            if (error.errorCode !in IO_ERRORS || !player.hasNextMediaItem() || ++failures >= player.mediaItemCount) return
            player.seekToNextMediaItem()
            player.prepare()
            player.play()
        }

        override fun onPlayWhenReadyChanged(
            playWhenReady: Boolean,
            reason: Int,
        ) {
            if (!playWhenReady && reason == Player.PLAY_WHEN_READY_CHANGE_REASON_END_OF_MEDIA_ITEM) sleep.onPausedAtEnd()
            updateWaiting()
        }

        // The queue changed, or its order: is the playing song still the last one?
        override fun onTimelineChanged(
            timeline: androidx.media3.common.Timeline,
            reason: Int,
        ) = sleep.update()

        override fun onShuffleModeEnabledChanged(shuffleModeEnabled: Boolean) = sleep.update()

        override fun onPlaybackStateChanged(state: Int) {
            if (state == Player.STATE_READY) loadFailed = false
            updateWaiting()
            if (state != Player.STATE_ENDED) return
            val s = current.value
            scope.launch {
                close("finished", s?.durationMs ?: player.currentPosition)
                if (s != null && isLong(s)) store.resume(s.id, null)
            }
        }
    }

    private fun updateWaiting() {
        waiting.value = loadFailed && player.playWhenReady && player.playbackState == Player.STATE_BUFFERING
    }

    companion object {
        const val MAX_QUEUES = 20

        /** A hand-off never starts closer than this to the song's end: the other device may have been closed mid-song. */
        private const val END_MARGIN_MS = 5_000L

        /** Media3's error codes 2000–2999 are input/output: the network, or a missing file. */
        private val IO_ERRORS = 2000..2999

        fun repeatMode(r: String) =
            when (r) {
                "queue" -> Player.REPEAT_MODE_ALL
                "song" -> Player.REPEAT_MODE_ONE
                else -> Player.REPEAT_MODE_OFF
            }
    }
}

/**
 * Always wait (docs/plans/019_networking_and_caching.md): Media3 gives up on
 * a song after four failed tries in about six seconds; this tries again
 * without limit, every few seconds at most, so a tunnel or a weak signal
 * holds the song up instead of losing it. An answer that cannot change by
 * trying again (no such song, not signed in) still fails at once.
 */
@OptIn(UnstableApi::class)
private object WaitForNetwork : DefaultLoadErrorHandlingPolicy() {
    override fun getRetryDelayMsFor(info: LoadErrorHandlingPolicy.LoadErrorInfo): Long {
        val code = (info.exception as? HttpDataSource.InvalidResponseCodeException)?.responseCode
        if (code != null && code in 400..499 && code != 408 && code != 429) return C.TIME_UNSET
        return super.getRetryDelayMsFor(info)
    }

    override fun getMinimumLoadableRetryCount(dataType: Int) = Int.MAX_VALUE
}
