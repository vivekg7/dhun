package io.github.vivekg7.dhun.data

import android.content.Context
import io.github.vivekg7.dhun.Prefs
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import okhttp3.Cache
import okhttp3.CacheControl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.io.File
import java.io.IOException
import java.time.OffsetDateTime
import java.util.concurrent.TimeUnit

/**
 * The client for api/openapi.yaml. One OkHttp client for the API, the
 * stream and the art, so the token is added in one place.
 */
class Api(
    context: Context,
    private val prefs: Prefs,
) {
    val json =
        Json {
            ignoreUnknownKeys = true
            explicitNulls = false
        }

    val http: OkHttpClient =
        OkHttpClient
            .Builder()
            // Art is fetched again and again while scrolling; the server sends ETags.
            .cache(Cache(File(context.cacheDir, "http"), 64L shl 20))
            .connectTimeout(10, TimeUnit.SECONDS)
            .readTimeout(30, TimeUnit.SECONDS)
            .addInterceptor { chain ->
                val req = chain.request()
                val token = prefs.token
                // Only to our own server: the token never leaves for another host.
                if (token.isEmpty() || !req.url.toString().startsWith(prefs.server)) {
                    chain.proceed(req)
                } else {
                    chain.proceed(req.newBuilder().header("Authorization", "Bearer $token").build())
                }
            }.build()

    fun streamUrl(song: Long) = "${prefs.server}/api/v1/stream/$song"

    fun artUrl(
        song: Long,
        size: Int,
    ) = "${prefs.server}/api/v1/art/$song?size=$size"

    suspend fun login(
        server: String,
        user: String,
        password: String,
        device: String,
    ): Login {
        val body =
            buildJsonObject {
                put("username", user)
                put("password", password)
                put("device", device)
            }
        return call(
            Request
                .Builder()
                .url("$server/api/v1/login")
                .post(body.toString().toRequestBody(JSON))
                .build(),
        )
    }

    suspend fun library(since: Long): Library = call(Request.Builder().url("${prefs.server}/api/v1/library?since=$since").build())

    suspend fun sync(
        since: Long,
        ops: List<JsonElement>,
    ): SyncState {
        val body =
            buildJsonObject {
                put("since", since)
                put("ops", kotlinx.serialization.json.JsonArray(ops))
            }
        return call(
            Request
                .Builder()
                .url("${prefs.server}/api/v1/sync")
                .post(body.toString().toRequestBody(JSON))
                .build(),
        )
    }

    suspend fun plays(): Plays = call(Request.Builder().url("${prefs.server}/api/v1/plays").build())

    private suspend inline fun <reified T> call(req: Request): T =
        withContext(Dispatchers.IO) {
            http.newCall(req).execute().use { resp ->
                val text = resp.body.string()
                if (!resp.isSuccessful) {
                    val msg = runCatching { json.decodeFromString<ApiError>(text).message }.getOrNull()
                    throw ApiException(resp.code, msg ?: "HTTP ${resp.code}")
                }
                json.decodeFromString<T>(text)
            }
        }

    companion object {
        private val JSON = "application/json".toMediaType()

        /**
         * For songs: the server sends Last-Modified and no Cache-Control,
         * which OkHttp would cache, pushing the art out of its 64 MB cache.
         */
        val NO_STORE: CacheControl = CacheControl.Builder().noStore().build()
    }
}

class ApiException(
    val code: Int,
    message: String,
) : IOException(message)

/** Server times are RFC 3339; the app keeps epoch milliseconds. */
fun parseTime(s: String): Long = if (s.isEmpty()) 0 else runCatching { OffsetDateTime.parse(s).toInstant().toEpochMilli() }.getOrDefault(0)

fun formatTime(ms: Long): String =
    java.time.Instant
        .ofEpochMilli(ms)
        .toString()

@Serializable data class ApiError(
    val code: String = "",
    val message: String = "",
)

@Serializable data class User(
    val id: Long = 0,
    val name: String = "",
    val admin: Boolean = false,
)

@Serializable data class Login(
    val token: String,
    val deviceId: Long = 0,
    val user: User = User(),
)

@Serializable
data class SongDto(
    val id: Long,
    val path: String = "",
    val title: String = "",
    val artist: String = "",
    val artists: List<String> = emptyList(),
    val album: String = "",
    val albumArtist: String = "",
    val composer: String = "",
    val genres: List<String> = emptyList(),
    val year: Int = 0,
    val track: Int = 0,
    val disc: Int = 0,
    val durationMs: Long = 0,
    val format: String = "",
    val bitrate: Int = 0,
    val sampleRate: Int = 0,
    val bitDepth: Int = 0,
    val size: Long = 0,
    val hasArt: Boolean = false,
    val hasLyrics: Boolean = false,
    val addedAt: String = "",
    val missing: Boolean = false,
) {
    fun toSong() =
        Song(
            id,
            path,
            title.ifEmpty { path.substringAfterLast('/').substringBeforeLast('.') },
            artist,
            artists.joinToString(SEP.toString()),
            album,
            albumArtist,
            composer,
            genres.joinToString(SEP.toString()),
            year,
            track,
            disc,
            durationMs,
            format,
            bitrate,
            sampleRate,
            bitDepth,
            size,
            hasArt,
            hasLyrics,
            parseTime(addedAt),
            missing,
        )
}

@Serializable
data class PlaylistDto(
    val id: Long,
    val deleted: Boolean = false,
    val path: String = "",
    val name: String = "",
    val shared: Boolean = false,
    val songs: List<Long> = emptyList(),
)

@Serializable data class Library(
    val version: Long = 0,
    val songs: List<SongDto> = emptyList(),
    val playlists: List<PlaylistDto> = emptyList(),
)

@Serializable
data class QueueDto(
    val id: String,
    val deleted: Boolean = false,
    val name: String = "",
    val songs: List<Long> = emptyList(),
    val currentSong: Long = 0,
    val positionMs: Long = 0,
    val shuffle: Boolean = false,
    val repeat: String = "off",
    val usedAt: String = "",
)

@Serializable data class ListItemDto(
    val song: Long,
    val deleted: Boolean = false,
    val at: String = "",
)

@Serializable data class ResumeDto(
    val song: Long,
    val deleted: Boolean = false,
    val positionMs: Long = 0,
    val at: String = "",
)

@Serializable
data class NowPlaying(
    val deviceId: Long = 0,
    val deviceName: String = "",
    val queue: String = "",
    val song: Long = 0,
    val positionMs: Long = 0,
    val playing: Boolean = false,
    val at: String = "",
)

@Serializable data class OpResult(
    val id: String,
    val status: String,
    val error: String = "",
)

@Serializable
data class SyncState(
    val version: Long = 0,
    val queues: List<QueueDto> = emptyList(),
    val favorites: List<ListItemDto> = emptyList(),
    val listenLater: List<ListItemDto> = emptyList(),
    val resume: List<ResumeDto> = emptyList(),
    val settings: Map<String, JsonElement> = emptyMap(),
    val nowPlaying: NowPlaying? = null,
    val results: List<OpResult> = emptyList(),
)

@Serializable data class PlayDto(
    val song: Long,
    val count: Int = 0,
    val lastPlayedAt: String = "",
)

@Serializable data class Plays(
    val plays: List<PlayDto> = emptyList(),
)

typealias Op = JsonObject
