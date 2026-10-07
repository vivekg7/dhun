package io.github.vivekg7.dhun.data

import io.github.vivekg7.dhun.App
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObjectBuilder
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import java.util.UUID

/**
 * Every change to the user's synced data goes through here: it is applied
 * to the working copy at once and recorded in the outbox, which [Sync]
 * pushes when the server is reachable. Offline edits are never lost
 * (AGENTS.md).
 */
class Store(
    private val app: App,
) {
    private val dao get() = app.db.dao()

    // One instance each: Compose re-subscribes when handed a new Flow object.
    val favorites: Flow<List<Mark>> = dao.marks(FAV)
    val listenLater: Flow<List<Mark>> = dao.marks(LATER)
    val resumes: Flow<List<Resume>> = dao.resumes()
    val playlists: Flow<List<Playlist>> = dao.playlists()
    val playStats: Flow<List<PlayStat>> = dao.playStats()
    val settings: Flow<Map<String, JsonElement>> =
        dao.settings().map { rows -> rows.associate { it.name to app.api.json.parseToJsonElement(it.value) } }

    suspend fun mark(
        kind: String,
        song: Long,
        on: Boolean,
    ) {
        val now = System.currentTimeMillis()
        dao.putMark(Mark(kind, song, now, deleted = !on))
        val type =
            when (kind) {
                FAV -> if (on) "favorite.set" else "favorite.unset"
                else -> if (on) "listen_later.add" else "listen_later.remove"
            }
        // Only the last toggle of a song matters.
        record(type, key = "$kind:$song") { put("song", song) }
    }

    suspend fun setting(
        name: String,
        value: JsonElement,
    ) {
        if (value is JsonNull) dao.deleteSetting(name) else dao.putSetting(Setting(name, value.toString()))
        record("setting.set", key = "setting:$name") {
            put("name", name)
            put("value", value)
        }
    }

    /** Where a long file was left, or null once it was heard to the end (plan 009). */
    suspend fun resume(
        song: Long,
        positionMs: Long?,
    ) {
        dao.putResume(Resume(song, positionMs ?: 0, System.currentTimeMillis(), deleted = positionMs == null))
        if (positionMs == null) {
            record("resume.unset", key = "resume:$song") { put("song", song) }
        } else {
            record("resume.set", key = "resume:$song") {
                put("song", song)
                put("positionMs", positionMs)
            }
        }
    }

    suspend fun putQueue(q: QueueRow) = dao.putQueue(q)

    suspend fun createQueue(q: QueueRow) {
        dao.putQueue(q)
        record("queue.create", queue = q.id) {
            put("queue", q.id)
            put("name", q.name)
            put("songs", ids(q.songs))
            put("song", q.currentSong)
            put("positionMs", q.positionMs)
        }
    }

    suspend fun replaceQueue(q: QueueRow) {
        dao.putQueue(q)
        record("queue.replace", key = "replace:${q.id}") {
            put("queue", q.id)
            put("songs", ids(q.songs))
        }
    }

    suspend fun renameQueue(q: QueueRow) {
        dao.putQueue(q)
        record("queue.rename", key = "rename:${q.id}") {
            put("queue", q.id)
            put("name", q.name)
        }
    }

    suspend fun deleteQueue(id: String) {
        dao.deleteQueue(id)
        record("queue.delete", queue = id) { put("queue", id) }
    }

    /** The queue's current song and position; only the latest is worth sending. */
    suspend fun setCurrent(q: QueueRow) {
        dao.putQueue(q)
        record("queue.set_current", key = "current:${q.id}") {
            put("queue", q.id)
            put("song", q.currentSong)
            put("positionMs", q.positionMs)
        }
    }

    suspend fun setMode(q: QueueRow) {
        dao.putQueue(q)
        record("queue.set_mode", key = "mode:${q.id}") {
            put("queue", q.id)
            put("shuffle", q.shuffle)
            put("repeat", q.repeat)
        }
    }

    suspend fun insertIntoQueue(
        q: QueueRow,
        songs: List<Long>,
        after: Long?,
    ) {
        dao.putQueue(q)
        record("queue.insert", queue = q.id) {
            put("queue", q.id)
            put("songs", JsonArray(songs.map(::JsonPrimitive)))
            if (after != null) put("after", after)
        }
    }

    suspend fun removeFromQueue(
        q: QueueRow,
        songs: List<Long>,
    ) {
        dao.putQueue(q)
        record("queue.remove", queue = q.id) {
            put("queue", q.id)
            put("songs", JsonArray(songs.map(::JsonPrimitive)))
        }
    }

    suspend fun moveInQueue(
        q: QueueRow,
        song: Long,
        after: Long,
    ) {
        dao.putQueue(q)
        record("queue.move", queue = q.id) {
            put("queue", q.id)
            put("song", song)
            put("after", after)
        }
    }

    // --- Playlists (docs/plans/013_playlist_editing.md) ----------------------

    /** A new playlist of the user's own; a negative id until the server answers with the same ref. */
    suspend fun createPlaylist(
        name: String,
        songs: List<Long>,
    ): Playlist {
        val p = Playlist(-(1L + kotlin.random.Random.nextLong(Long.MAX_VALUE - 1)), name, "", false, songs.distinct().joinIds(), UUID.randomUUID().toString())
        dao.putPlaylist(p)
        record("playlist.create", playlist = p.id) {
            put("ref", p.ref)
            put("name", p.name)
            put("songs", ids(p.songs))
        }
        return p
    }

    /** Adds [songs] at the end, skipping those already in it; returns how many were skipped. */
    suspend fun addToPlaylist(
        p: Playlist,
        songs: List<Long>,
    ): Int {
        val have = songIds(p.songs).toSet()
        val adding = songs.distinct().filter { it !in have }
        if (adding.isNotEmpty()) {
            dao.putPlaylist(p.copy(songs = (songIds(p.songs) + adding).joinIds()))
            record("playlist.insert", playlist = p.id) {
                put("playlist", p.target)
                put("songs", JsonArray(adding.map(::JsonPrimitive)))
            }
        }
        return songs.distinct().size - adding.size
    }

    /** Removes the entry at [index] of the playlist's own list (unmatched entries included). */
    suspend fun removeFromPlaylist(
        p: Playlist,
        index: Int,
    ) {
        val list = songIds(p.songs).toMutableList()
        if (index !in list.indices) return
        val song = list[index]
        val occurrence = list.subList(0, index).count { it == song }
        list.removeAt(index)
        dao.putPlaylist(p.copy(songs = list.joinIds()))
        record("playlist.remove", playlist = p.id) {
            put("playlist", p.target)
            put("song", song)
            put("occurrence", occurrence)
        }
    }

    /** Moves the entry at [from] to [to], both indexes into the playlist's own list. */
    suspend fun moveInPlaylist(
        p: Playlist,
        from: Int,
        to: Int,
    ) {
        val list = songIds(p.songs).toMutableList()
        if (from !in list.indices || to !in list.indices || from == to) return
        val song = list[from]
        val occurrence = list.subList(0, from).count { it == song }
        list.add(to, list.removeAt(from))
        dao.putPlaylist(p.copy(songs = list.joinIds()))
        val after = if (to == 0) 0L else list[to - 1]
        if (to > 0 && after == 0L) {
            // An unmatched entry cannot be named as "after" (0 means the start): send the order.
            record("playlist.replace", playlist = p.id) {
                put("playlist", p.target)
                put("songs", JsonArray(list.map(::JsonPrimitive)))
            }
            return
        }
        record("playlist.move", playlist = p.id) {
            put("playlist", p.target)
            put("song", song)
            put("occurrence", occurrence)
            put("after", after)
            if (to > 0) put("afterOccurrence", list.subList(0, to - 1).count { it == after })
        }
    }

    suspend fun renamePlaylist(
        p: Playlist,
        name: String,
    ) {
        dao.putPlaylist(p.copy(name = name))
        record("playlist.rename", playlist = p.id) {
            put("playlist", p.target)
            put("name", name)
        }
    }

    /** The server keeps a copy of the file; a download of it goes too. */
    suspend fun deletePlaylist(p: Playlist) {
        dao.deletePlaylist(p.id)
        dao.deletePin(Downloads.key(Downloads.PLAYLIST, p.id.toString()))
        record("playlist.delete", playlist = p.id) { put("playlist", p.target) }
    }

    /** Feeds /now-playing, for "Continue from …" on another device. */
    suspend fun playbackState(
        queue: String,
        song: Long,
        positionMs: Long,
        playing: Boolean,
    ) = record("playback.state", key = "playback") {
        put("queue", queue)
        put("song", song)
        put("positionMs", positionMs)
        put("playing", playing)
    }

    /** One listen (docs/plans/008_listening_history.md). */
    suspend fun play(l: Listen) =
        record("play", at = l.startedAt) {
            put("song", l.song)
            put("ms", l.ms)
            put("endedAt", formatTime(l.endedAt))
            put("fromMs", l.fromMs)
            put("toMs", l.toMs)
            put("end", l.end)
            if (l.source.isNotEmpty()) put("source", l.source)
            if (l.queue.isNotEmpty()) put("queue", l.queue)
            put("shuffle", l.shuffle)
            put("utcOffset", l.utcOffset)
        }

    /**
     * [key] replaces earlier ops with the same key. [queue] and [playlist]
     * mark an op as touching that queue or playlist, so [Sync] does not
     * overwrite it with an older server copy while the op is still unsent.
     */
    private suspend fun record(
        type: String,
        key: String? = null,
        queue: String? = null,
        playlist: Long? = null,
        at: Long = System.currentTimeMillis(),
        fields: JsonObjectBuilder.() -> Unit,
    ) {
        val id = UUID.randomUUID().toString()
        val op =
            buildJsonObject {
                put("id", id)
                put("type", type)
                put("at", formatTime(at))
                fields()
            }
        if (key != null) dao.dropOps(key)
        val tag =
            when {
                queue != null -> ":$queue"
                playlist != null -> ":${playlistTag(playlist)}"
                else -> ""
            }
        dao.addOp(OutboxOp(id = id, key = key ?: "$id$tag", json = op.toString()))
        app.sync.soon()
    }

    companion object {
        const val FAV = "fav"
        const val LATER = "later"

        fun ids(text: String) = JsonArray(songIds(text).map(::JsonPrimitive))

        /** Ends the outbox key of an op on that playlist. */
        fun playlistTag(id: Long) = "pl$id"
    }
}

fun songIds(text: String): List<Long> = if (text.isEmpty()) emptyList() else text.split(',').map { it.toLong() }

fun List<Long>.joinIds() = joinToString(",")

/** A synced setting read with the app's default for missing or null (plan 009). */
fun Map<String, JsonElement>.int(
    name: String,
    def: Int,
) = (this[name] as? JsonPrimitive)?.intOrNull ?: def

fun Map<String, JsonElement>.bool(
    name: String,
    def: Boolean,
) = (this[name] as? JsonPrimitive)?.booleanOrNull ?: def

fun Map<String, JsonElement>.string(
    name: String,
    def: String,
) = (this[name] as? JsonPrimitive)?.takeIf { it.isString }?.jsonPrimitive?.content ?: def

data class Listen(
    val song: Long,
    val startedAt: Long,
    val endedAt: Long,
    val ms: Long,
    val fromMs: Long,
    val toMs: Long,
    val end: String,
    val source: String,
    val queue: String,
    val shuffle: Boolean,
    val utcOffset: Int,
)
