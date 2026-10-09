package io.github.vivekg7.dhun.data

import android.Manifest
import android.content.ContentUris
import android.content.pm.PackageManager
import android.database.ContentObserver
import android.graphics.Bitmap
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.MediaStore
import android.util.Size
import androidx.core.net.toUri
import io.github.vivekg7.dhun.App
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.security.MessageDigest
import java.util.concurrent.ConcurrentHashMap

private typealias M = MediaStore.Audio.Media

/**
 * Songs already on the phone (docs/plans/025_phone_local_songs.md), read
 * from Android's media index into memory and shown beside the NAS songs.
 * Their IDs are negative, so they fit every list of song IDs as they are,
 * and nothing with a negative ID is ever sent to the server ([Store]).
 */
class LocalSongs(
    private val app: App,
) {
    private val _songs = MutableStateFlow<List<Song>?>(null)

    /** Null until the first read, so a queue holding phone songs is not loaded without them. */
    val songs: StateFlow<List<Song>?> = _songs

    /** Every folder holding audio, excluded ones too, with how many files: what Settings offers to exclude. */
    val folders = MutableStateFlow<List<Pair<String, Int>>>(emptyList())

    private val uris = ConcurrentHashMap<Long, Uri>()
    private val noArt = ConcurrentHashMap.newKeySet<Long>()
    private var pending: Job? = null

    init {
        app.contentResolver.registerContentObserver(
            MediaStore.Audio.Media.getContentUri(MediaStore.VOLUME_EXTERNAL),
            true,
            object : ContentObserver(Handler(Looper.getMainLooper())) {
                override fun onChange(selfChange: Boolean) = refresh(delayMs = 2_000)
            },
        )
        refresh(delayMs = 0)
    }

    fun allowed() = app.checkSelfPermission(PERMISSION) == PackageManager.PERMISSION_GRANTED

    /** Reads the index again; debounced, since copying an album in fires once per file. */
    fun refresh(delayMs: Long = 0) {
        pending?.cancel()
        pending =
            app.scope.launch(Dispatchers.IO) {
                delay(delayMs)
                _songs.value = if (app.prefs.phoneSongs && allowed()) read() else emptyList<Song>().also { folders.value = emptyList() }
            }
    }

    fun uri(id: Long): Uri? = uris[id]

    /** A phone song's cover as an image the media session can load. */
    fun artUri(song: Song): Uri = ContentUris.withAppendedId(ALBUM_ART, song.art.toLongOrNull() ?: 0)

    /** The file's own cover, as Android's thumbnailer finds it; null when it has none. */
    suspend fun cover(
        id: Long,
        px: Int,
    ): Bitmap? {
        val uri = uris[id] ?: return null
        if (id in noArt) return null
        return withContext(Dispatchers.IO) {
            runCatching { app.contentResolver.loadThumbnail(uri, Size(px, px), null) }.getOrNull().also { if (it == null) noArt += id }
        }
    }

    private fun read(): List<Song> {
        val prefs = app.prefs
        val minMs = prefs.phoneMinSeconds * 1000L
        val excluded = prefs.phoneExcluded
        val out = ArrayList<Song>()
        val counts = HashMap<String, Int>()
        val found = HashMap<Long, Uri>()
        val base = MediaStore.Audio.Media.getContentUri(MediaStore.VOLUME_EXTERNAL)
        // Ringtones, alarms and recordings are audio but not music, whatever their length.
        val selection = "${M.IS_RINGTONE} = 0 AND ${M.IS_NOTIFICATION} = 0 AND ${M.IS_ALARM} = 0 AND ${M.IS_RECORDING} = 0"
        app.contentResolver.query(base, COLUMNS, selection, null, null)?.use { c ->
            fun str(col: String) = c.getString(c.getColumnIndexOrThrow(col)).orEmpty().let { if (it == "<unknown>") "" else it }

            fun int(col: String) = c.getInt(c.getColumnIndexOrThrow(col))

            fun long(col: String) = c.getLong(c.getColumnIndexOrThrow(col))
            while (c.moveToNext()) {
                val volume = str(M.VOLUME_NAME)
                val name = str(M.DISPLAY_NAME)
                // "On this phone/Music/Album/", with a memory card's songs under its own name.
                val folder = (PHONE_ROOT + "/" + (if (volume == MediaStore.VOLUME_EXTERNAL_PRIMARY) "" else "$volume/") + str(M.RELATIVE_PATH)).trimEnd('/')
                counts[folder] = (counts[folder] ?: 0) + 1
                val duration = long(M.DURATION)
                if (duration < minMs || excluded.any { folder == it || folder.startsWith("$it/") }) continue
                val id = phoneId("$volume/${str(M.RELATIVE_PATH)}$name")
                found[id] = ContentUris.withAppendedId(base, long(M._ID))
                val track = int(M.TRACK)
                val artist = str(M.ARTIST)
                val genre = str(M.GENRE)
                out +=
                    Song(
                        id = id,
                        path = "$folder/$name",
                        title = str(M.TITLE).ifEmpty { name.substringBeforeLast('.') },
                        artist = artist,
                        artists = artist,
                        album = str(M.ALBUM),
                        albumArtist = str(M.ALBUM_ARTIST),
                        composer = str(M.COMPOSER),
                        genres = genre,
                        year = int(M.YEAR),
                        // Android keeps the disc in the thousands: 2003 is disc 2, track 3.
                        track = track % 1000,
                        disc = track / 1000,
                        durationMs = duration,
                        format = str(M.MIME_TYPE).substringAfter('/'),
                        bitrate = int(M.BITRATE) / 1000,
                        sampleRate = 0,
                        bitDepth = 0,
                        size = long(M.SIZE),
                        hasArt = true,
                        hasLyrics = false,
                        addedAt = long(M.DATE_ADDED) * 1000,
                        missing = false,
                        // Android's album, for the notification's cover (Playback).
                        art = long(M.ALBUM_ID).toString(),
                    )
            }
        }
        uris.keys.retainAll(found.keys)
        uris.putAll(found)
        folders.value = counts.entries.sortedWith(compareBy(Catalog.TITLE) { it.key }).map { it.key to it.value }
        return out
    }

    companion object {
        /** The Folders tab's root for phone songs, beside the NAS folders. */
        const val PHONE_ROOT = "On this phone"

        private val ALBUM_ART = "content://media/external/audio/albumart".toUri()

        val PERMISSION = if (Build.VERSION.SDK_INT >= 33) Manifest.permission.READ_MEDIA_AUDIO else Manifest.permission.READ_EXTERNAL_STORAGE

        private val COLUMNS =
            arrayOf(
                M._ID,
                M.VOLUME_NAME,
                M.RELATIVE_PATH,
                M.DISPLAY_NAME,
                M.TITLE,
                M.ARTIST,
                M.ALBUM,
                M.ALBUM_ARTIST,
                M.COMPOSER,
                M.GENRE,
                M.YEAR,
                M.TRACK,
                M.DURATION,
                M.MIME_TYPE,
                M.BITRATE,
                M.SIZE,
                M.DATE_ADDED,
                M.ALBUM_ID,
            )
    }
}

/**
 * A phone song's ID: from where the file is, not Android's own ID, which
 * changes when Android rebuilds its media index. Favourites and counts so
 * survive that, and come back with a file that went missing. Always
 * negative: 63 bits of a SHA-256.
 */
fun phoneId(place: String): Long {
    val d = MessageDigest.getInstance("SHA-256").digest(place.toByteArray())
    var h = 0L
    for (i in 0 until 8) h = (h shl 8) or (d[i].toLong() and 0xff)
    return -(h ushr 1) - 1
}
