package io.github.vivekg7.dhun.data

import io.github.vivekg7.dhun.Prefs
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.withContext
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.decodeFromStream
import kotlinx.serialization.json.put
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.io.IOException
import java.time.Instant
import java.time.LocalDate
import java.time.OffsetDateTime
import java.util.concurrent.TimeUnit

/**
 * The client for api/openapi.yaml. One OkHttp client for the API, the
 * stream and the art, so the token is added in one place.
 */
class Api(
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
            .connectTimeout(10, TimeUnit.SECONDS)
            .readTimeout(30, TimeUnit.SECONDS)
            .addInterceptor { chain ->
                val req = chain.request()
                val token = prefs.token
                // Only to our own server: the token never leaves for another host.
                if (token.isEmpty() || !req.url.toString().startsWith(prefs.server)) {
                    chain.proceed(req)
                } else {
                    val res =
                        try {
                            chain.proceed(req.newBuilder().header("Authorization", "Bearer $token").build())
                        } catch (e: IOException) {
                            if (!chain.call().isCanceled()) reachable.value = false
                            throw e
                        }
                    reachable.value = true
                    // The server's release, for Settings; sent only to a signed-in device.
                    res.header("Dhun-Version")?.let { if (it != prefs.serverVersion) prefs.serverVersion = it }
                    res
                }
            }.build()

    /**
     * For songs, the player's stream and the cache's fetches: a stall is
     * given up after 10 s, not 30, and tried again; the next try may well
     * work (docs/plans/019_networking_and_caching.md).
     */
    val songs: OkHttpClient by lazy { http.newBuilder().readTimeout(10, TimeUnit.SECONDS).build() }

    /**
     * False while the server cannot be reached: the last request to it failed
     * and none has worked since, or the phone has no network ([Sync]). Songs
     * not on the phone are dimmed then (docs/plans/012_downloads.md). Every
     * request counts, a song's bytes included, so one that works brings the
     * app back at once (docs/plans/019_networking_and_caching.md).
     */
    val reachable = MutableStateFlow(true)

    fun streamUrl(song: Long) = url("stream/$song")

    fun artUrl(
        song: Long,
        size: Int,
    ) = url("art/$song?size=$size")

    suspend fun login(
        server: String,
        user: String,
        password: String,
        device: String,
    ): Login =
        send(
            "$server/api/v1/login",
            buildJsonObject {
                put("username", user)
                put("password", password)
                put("device", device)
            },
        )

    /** Podcasts and audiobooks too (`kinds=all`): without it the server sends music only, for apps that predate them. */
    suspend fun library(since: Long): Library = get("library?since=$since&kinds=all")

    suspend fun sync(
        since: Long,
        ops: List<JsonElement>,
    ): SyncState =
        send(
            url("sync"),
            buildJsonObject {
                put("since", since)
                put("ops", JsonArray(ops))
            },
        )

    /** Thumbnails by art key, base64; "" for a key with nothing to show. At most [MAX_THUMBS] keys. */
    suspend fun thumbs(keys: List<String>): Map<String, String> =
        send<ThumbsDto>(url("thumbs"), buildJsonObject { put("keys", JsonArray(keys.map(::JsonPrimitive))) }).thumbs

    suspend fun lyrics(song: Long): LyricsDto = get("lyrics/$song")

    /** Songs whose lyrics hold every word of [q], each with the line that does (docs/plans/029_search.md). */
    suspend fun searchLyrics(q: String): List<LyricsHit> =
        call<LyricsHits>(
            Request
                .Builder()
                .url(
                    url("search/lyrics")
                        .toHttpUrl()
                        .newBuilder()
                        .addQueryParameter("q", q)
                        .build(),
                ).build(),
        ).hits

    suspend fun me(): Me = get("me")

    suspend fun nowPlaying(): NowPlaying? = get<NowPlayingResponse>("now-playing").nowPlaying

    suspend fun plays(): Plays = get("plays")

    // Family members, for the admin (docs/plans/023_users_on_android.md).

    suspend fun members(): List<Member> = get<Members>("admin/users").users

    suspend fun addMember(
        name: String,
        password: String,
    ) {
        send<JsonObject>(
            url("admin/users"),
            buildJsonObject {
                put("name", name)
                put("password", password)
            },
        )
    }

    suspend fun resetPassword(
        user: Long,
        password: String,
    ) = send<Unit>(url("admin/users/$user/password"), buildJsonObject { put("password", password) }, "PUT")

    private fun url(path: String) = "${prefs.server}/api/v1/$path"

    private suspend inline fun <reified T> get(path: String): T = call(Request.Builder().url(url(path)).build())

    private suspend inline fun <reified T> send(
        url: String,
        body: JsonObject,
        method: String = "POST",
    ): T =
        call(
            Request
                .Builder()
                .url(url)
                .method(method, body.toString().toRequestBody(JSON))
                .build(),
        )

    /**
     * Decoded from the stream: the first library sync is tens of megabytes
     * of JSON at 100,000 songs, and read whole it would sit on the heap
     * beside the objects made from it (docs/plans/036_fast_at_100k.md).
     */
    @OptIn(ExperimentalSerializationApi::class)
    private suspend inline fun <reified T> call(req: Request): T =
        withContext(Dispatchers.IO) {
            http.newCall(req).execute().use { resp ->
                if (!resp.isSuccessful) {
                    val msg = runCatching { json.decodeFromString<ApiError>(resp.body.string()).message }.getOrNull()
                    throw ApiException(resp.code, msg ?: "HTTP ${resp.code}")
                }
                // A 204 has no body to decode.
                if (T::class == Unit::class) Unit as T else json.decodeFromStream<T>(resp.body.byteStream())
            }
        }

    companion object {
        private val JSON = "application/json".toMediaType()

        /** The server's limit for one thumbnails request. */
        const val MAX_THUMBS = 50
    }
}

class ApiException(
    val code: Int,
    message: String,
) : IOException(message)

/**
 * Server times are RFC 3339; the app keeps epoch milliseconds. The server
 * writes them in UTC with a `Z`, with or without fractional seconds, and
 * that form is read by hand: a date-time parser per song is a visible
 * share of a 100,000-song pull. Anything else goes through the library.
 */
fun parseTime(s: String): Long {
    if (s.isEmpty()) return 0
    return utcTime(s) ?: runCatching { OffsetDateTime.parse(s).toInstant().toEpochMilli() }.getOrDefault(0)
}

/** `2006-01-02T15:04:05Z` or `2006-01-02T15:04:05.000Z` as epoch milliseconds; null for anything else. */
private fun utcTime(s: String): Long? {
    if (s.length < 20 || s[4] != '-' || s[7] != '-' || s[10] != 'T' || s[13] != ':' || s[16] != ':' || s.last() != 'Z') return null

    fun num(
        from: Int,
        to: Int,
    ): Int {
        var n = 0
        for (i in from until to) {
            val d = s[i] - '0'
            if (d !in 0..9) return -1
            n = n * 10 + d
        }
        return n
    }
    val year = num(0, 4)
    val month = num(5, 7)
    val day = num(8, 10)
    val hour = num(11, 13)
    val minute = num(14, 16)
    val second = num(17, 19)
    if (year < 0 || month !in 1..12 || day !in 1..31 || hour !in 0..23 || minute !in 0..59 || second !in 0..60) return null
    var millis = 0L
    if (s.length > 20) {
        if (s[19] != '.') return null
        // Up to three places count; the rest are finer than a millisecond.
        var scale = 100
        for (i in 20 until s.length - 1) {
            val d = s[i] - '0'
            if (d !in 0..9) return null
            millis += d * scale
            scale /= 10
        }
    }
    // A day the month does not have (30 February) is no time at all.
    val days = runCatching { LocalDate.of(year, month, day).toEpochDay() }.getOrNull() ?: return 0
    return ((days * 86_400 + hour * 3_600 + minute * 60 + second) * 1_000) + millis
}

fun formatTime(ms: Long): String = Instant.ofEpochMilli(ms).toString()

@Serializable data class ApiError(
    val code: String = "",
    val message: String = "",
)

@Serializable data class User(
    val id: Long = 0,
    val name: String = "",
    val admin: Boolean = false,
)

@Serializable data class Member(
    val id: Long = 0,
    val name: String = "",
    val admin: Boolean = false,
    val devices: Int = 0,
    val lastSeenAt: String = "",
)

@Serializable data class Members(
    val users: List<Member> = emptyList(),
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
    val art: String = "",
    val hasLyrics: Boolean = false,
    val addedAt: String = "",
    val missing: Boolean = false,
    // Podcasts and audiobooks only (docs/plans/031_podcasts_and_audiobooks.md); no kind is music.
    val kind: String = "",
    val date: String = "",
    val notes: String = "",
    val chapters: List<Chapter> = emptyList(),
    val group: String = "",
    val groupTitle: String = "",
    val groupIndex: Int = 0,
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
            art,
            kind,
            date,
            notes,
            if (chapters.isEmpty()) "" else Json.encodeToString(chapters),
            group,
            groupTitle,
            groupIndex,
        )
}

/** Where a chapter of an episode or a book's file starts. */
@Serializable data class Chapter(
    val startMs: Long = 0,
    val title: String = "",
) {
    /** Its title, or "" when that is only its number ("001" in Dune), which the app shows anyway. */
    val name get() = title.takeUnless { it.all(Char::isDigit) }.orEmpty()
}

@Serializable
data class PlaylistDto(
    val id: Long,
    val deleted: Boolean = false,
    val path: String = "",
    val name: String = "",
    val shared: Boolean = false,
    val ref: String = "",
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

@Serializable data class BookmarkDto(
    val id: String,
    val deleted: Boolean = false,
    val song: Long = 0,
    val positionMs: Long = 0,
    val note: String = "",
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

@Serializable data class NowPlayingResponse(
    val nowPlaying: NowPlaying? = null,
)

@Serializable data class Me(
    val user: User = User(),
    val deviceId: Long = 0,
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
    val played: List<ListItemDto> = emptyList(),
    val bookmarks: List<BookmarkDto> = emptyList(),
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

@Serializable data class ThumbsDto(
    val thumbs: Map<String, String> = emptyMap(),
)

@Serializable data class LyricsDto(
    val source: String = "",
    val synced: Boolean = false,
    val text: String = "",
)

@Serializable data class LyricsHit(
    val song: Long = 0,
    val line: String = "",
)

@Serializable data class LyricsHits(
    val hits: List<LyricsHit> = emptyList(),
)

typealias Op = JsonObject
