package io.github.vivekg7.dhun.data

import android.content.Context
import androidx.room3.AutoMigration
import androidx.room3.ColumnInfo
import androidx.room3.Dao
import androidx.room3.Database
import androidx.room3.Entity
import androidx.room3.Insert
import androidx.room3.PrimaryKey
import androidx.room3.Query
import androidx.room3.Room
import androidx.room3.RoomDatabase
import androidx.room3.Upsert
import androidx.sqlite.driver.AndroidSQLiteDriver
import kotlinx.coroutines.flow.Flow

/**
 * The working copy ([docs/plans/006_api_and_sync.md]): the catalogue, the
 * user's synced data, and the outbox of changes not yet on the server.
 * Lists of song IDs are stored as comma-separated text: they are only ever
 * read and written whole, so a join table would buy nothing.
 */
@Database(
    entities = [
        Song::class, Playlist::class, QueueRow::class, Mark::class, Resume::class, Setting::class, OutboxOp::class, PlayStat::class,
        Pin::class, Download::class, LyricsRow::class, Thumb::class,
    ],
    version = 6,
    // Migrations are generated from the exported schemas, so an upgrade
    // keeps the outbox: offline edits are never lost (AGENTS.md).
    autoMigrations = [
        AutoMigration(
            from = 1,
            to = 2,
        ), AutoMigration(from = 2, to = 3), AutoMigration(from = 3, to = 4), AutoMigration(from = 4, to = 5), AutoMigration(from = 5, to = 6),
    ],
)
abstract class Db : RoomDatabase() {
    abstract fun dao(): DbDao

    companion object {
        fun open(context: Context): Db =
            Room
                .databaseBuilder(context, Db::class.java, "dhun.db")
                // The platform's SQLite: Room's bundled one is native code we don't need.
                .setDriver(AndroidSQLiteDriver())
                // Only for a schema with no migration path; everything but the
                // outbox can be fetched again from the server.
                .fallbackToDestructiveMigration(dropAllTables = true)
                .build()
    }
}

/** Joins and splits the text form of a list (artists, genres). */
const val SEP = '\u001f'

@Entity(tableName = "song")
data class Song(
    @PrimaryKey val id: Long,
    val path: String,
    val title: String,
    val artist: String,
    val artists: String,
    val album: String,
    val albumArtist: String,
    val composer: String,
    val genres: String,
    val year: Int,
    val track: Int,
    val disc: Int,
    val durationMs: Long,
    val format: String,
    val bitrate: Int,
    val sampleRate: Int,
    val bitDepth: Int,
    val size: Long,
    val hasArt: Boolean,
    val hasLyrics: Boolean,
    val addedAt: Long,
    val missing: Boolean,
    /** Which cover it shows, shared by songs showing the same one ([Covers]); empty from a server without it. */
    @ColumnInfo(defaultValue = "") val art: String = "",
) {
    val artistList get() = if (artists.isEmpty()) emptyList() else artists.split(SEP)
    val genreList get() = if (genres.isEmpty()) emptyList() else genres.split(SEP)
    val folder get() = path.substringBeforeLast('/', "")
    val displayArtist get() = artist.ifEmpty { albumArtist }.ifEmpty { "Unknown artist" }

    /** A song already on the phone, never sent to the server ([LocalSongs]). */
    val onPhone get() = id < 0
}

/**
 * A playlist as the server indexed it. One made on this phone has a negative
 * [id] until the server's comes back with the same [ref]
 * (docs/plans/013_playlist_editing.md).
 */
@Entity(tableName = "playlist")
data class Playlist(
    @PrimaryKey val id: Long,
    val name: String,
    val path: String,
    val shared: Boolean,
    val songs: String,
    @ColumnInfo(defaultValue = "") val ref: String = "",
) {
    /** How ops name it: the server's id, or the ref while the server has not answered. */
    val target get() = if (id > 0) id.toString() else "ref:$ref"
}

@Entity(tableName = "queue")
data class QueueRow(
    @PrimaryKey val id: String,
    val name: String,
    val songs: String,
    val currentSong: Long,
    val positionMs: Long,
    val shuffle: Boolean,
    val repeat: String,
    val usedAt: Long,
)

/** One entry of Favorites ("fav") or Listen Later ("later"); [at] decides between two edits. */
@Entity(tableName = "mark", primaryKeys = ["kind", "song"])
data class Mark(
    val kind: String,
    val song: Long,
    val at: Long,
    val deleted: Boolean,
)

@Entity(tableName = "resume")
data class Resume(
    @PrimaryKey val song: Long,
    val positionMs: Long,
    val at: Long,
    val deleted: Boolean,
)

/** A synced setting, as the JSON text the server returned. */
@Entity(tableName = "setting")
data class Setting(
    @PrimaryKey val name: String,
    val value: String,
)

/**
 * A change not yet acknowledged by the server, as the JSON op it sends.
 * [key] groups ops where only the latest matters (where a long file was
 * left, the current song of a queue), so a long offline session does not
 * pile them up.
 */
@Entity(tableName = "outbox")
data class OutboxOp(
    @PrimaryKey(autoGenerate = true) val seq: Long = 0,
    val id: String,
    val key: String,
    val json: String,
)

@Entity(tableName = "play_stat")
data class PlayStat(
    @PrimaryKey val song: Long,
    val count: Int,
    val lastPlayedAt: Long,
)

/**
 * Something the user downloaded, kept in step with the server
 * (docs/plans/012_downloads.md): [kind] is song, album, folder, artist,
 * genre, playlist or list, and [ref] says which one.
 */
@Entity(tableName = "pin")
data class Pin(
    @PrimaryKey val key: String,
    val kind: String,
    val ref: String,
    val name: String,
    val at: Long,
)

/** A downloaded file. [size] is the catalogue's size when it was fetched, to notice an upgraded file. */
@Entity(tableName = "download")
data class Download(
    @PrimaryKey val song: Long,
    val path: String,
    val size: Long,
    val at: Long,
)

/**
 * A cover's 128 px thumbnail, by art key ([Covers]): one for every cover in
 * the library, so list rows never wait for art. Empty [data] means the
 * server has nothing to show for that key.
 */
@Entity(tableName = "thumb")
class Thumb(
    @PrimaryKey val key: String,
    val data: ByteArray,
)

/** A song's lyrics as the server sent them, kept for offline (docs/plans/015_lyrics.md). */
@Entity(tableName = "lyrics")
data class LyricsRow(
    @PrimaryKey val song: Long,
    val text: String,
    val at: Long,
)

@Dao
interface DbDao {
    @Query("SELECT * FROM song WHERE missing = 0")
    fun songs(): Flow<List<Song>>

    @Upsert
    suspend fun putSongs(songs: List<Song>)

    @Query("SELECT * FROM playlist ORDER BY shared, name COLLATE NOCASE")
    fun playlists(): Flow<List<Playlist>>

    @Upsert
    suspend fun putPlaylist(p: Playlist)

    @Query("DELETE FROM playlist WHERE id = :id")
    suspend fun deletePlaylist(id: Long)

    @Query("SELECT * FROM playlist WHERE id = :id")
    suspend fun playlist(id: Long): Playlist?

    @Query("SELECT * FROM playlist WHERE id < 0")
    suspend fun localPlaylists(): List<Playlist>

    @Query("UPDATE pin SET `key` = :key, ref = :ref WHERE `key` = :old")
    suspend fun movePin(
        old: String,
        key: String,
        ref: String,
    )

    @Query("SELECT * FROM queue ORDER BY usedAt DESC")
    fun queues(): Flow<List<QueueRow>>

    @Query("SELECT * FROM queue WHERE id = :id")
    suspend fun queue(id: String): QueueRow?

    @Upsert
    suspend fun putQueue(q: QueueRow)

    @Query("DELETE FROM queue WHERE id = :id")
    suspend fun deleteQueue(id: String)

    @Query("SELECT * FROM mark WHERE kind = :kind AND deleted = 0 ORDER BY at DESC")
    fun marks(kind: String): Flow<List<Mark>>

    @Query("SELECT * FROM mark WHERE kind = :kind AND song = :song")
    suspend fun mark(
        kind: String,
        song: Long,
    ): Mark?

    @Upsert
    suspend fun putMark(m: Mark)

    @Query("SELECT * FROM resume WHERE deleted = 0 ORDER BY at DESC")
    fun resumes(): Flow<List<Resume>>

    @Query("SELECT * FROM resume WHERE song = :song")
    suspend fun resume(song: Long): Resume?

    @Upsert
    suspend fun putResume(r: Resume)

    @Query("SELECT * FROM setting")
    fun settings(): Flow<List<Setting>>

    @Upsert
    suspend fun putSetting(s: Setting)

    @Query("DELETE FROM setting WHERE name = :name")
    suspend fun deleteSetting(name: String)

    @Query("SELECT * FROM outbox ORDER BY seq LIMIT :limit")
    suspend fun outbox(limit: Int): List<OutboxOp>

    @Query("SELECT count(*) FROM outbox")
    suspend fun outboxSize(): Int

    @Insert
    suspend fun addOp(op: OutboxOp)

    @Query("DELETE FROM outbox WHERE key = :key")
    suspend fun dropOps(key: String)

    @Query("DELETE FROM outbox WHERE id IN (:ids)")
    suspend fun ackOps(ids: List<String>)

    @Query("SELECT * FROM play_stat")
    fun playStats(): Flow<List<PlayStat>>

    @Upsert
    suspend fun putPlayStats(stats: List<PlayStat>)

    // Phone songs' counts are kept on the phone only (docs/plans/025_phone_local_songs.md).
    @Query("DELETE FROM play_stat WHERE song > 0")
    suspend fun clearPlayStats()

    @Query("SELECT * FROM play_stat WHERE song = :song")
    suspend fun playStat(song: Long): PlayStat?

    @Query("SELECT * FROM pin ORDER BY at DESC")
    fun pins(): Flow<List<Pin>>

    @Upsert
    suspend fun putPin(p: Pin)

    @Query("DELETE FROM pin WHERE key = :key")
    suspend fun deletePin(key: String)

    @Query("SELECT * FROM download")
    fun downloads(): Flow<List<Download>>

    @Upsert
    suspend fun putDownload(d: Download)

    @Query("DELETE FROM download WHERE song = :song")
    suspend fun deleteDownload(song: Long)

    @Query("SELECT * FROM lyrics WHERE song = :song")
    suspend fun lyrics(song: Long): LyricsRow?

    @Query("SELECT song FROM lyrics")
    suspend fun lyricsSongs(): List<Long>

    @Upsert
    suspend fun putLyrics(l: LyricsRow)

    @Query("DELETE FROM lyrics WHERE song = :song")
    suspend fun deleteLyrics(song: Long)

    @Query("SELECT data FROM thumb WHERE `key` = :key")
    suspend fun thumb(key: String): ByteArray?

    @Query("SELECT DISTINCT art FROM song WHERE missing = 0 AND hasArt = 1 AND art != ''")
    suspend fun artKeys(): List<String>

    @Query("SELECT `key` FROM thumb")
    suspend fun thumbKeys(): List<String>

    @Upsert
    suspend fun putThumbs(t: List<Thumb>)

    @Query("DELETE FROM thumb WHERE `key` IN (:keys)")
    suspend fun deleteThumbs(keys: List<String>)
}
