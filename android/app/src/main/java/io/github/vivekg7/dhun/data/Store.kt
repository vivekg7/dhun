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
     * [key] replaces earlier ops with the same key. [queue] marks an op as
     * touching that queue, so [Sync] does not overwrite it with an older
     * server copy while the op is still unsent.
     */
    private suspend fun record(
        type: String,
        key: String? = null,
        queue: String? = null,
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
        dao.addOp(OutboxOp(id = id, key = key ?: if (queue != null) "$id:$queue" else id, json = op.toString()))
        app.sync.soon()
    }

    companion object {
        const val FAV = "fav"
        const val LATER = "later"

        fun ids(text: String) = JsonArray(songIds(text).map(::JsonPrimitive))
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
