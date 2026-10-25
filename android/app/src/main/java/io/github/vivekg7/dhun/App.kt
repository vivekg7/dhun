package io.github.vivekg7.dhun

import android.app.Application
import android.content.Context
import android.content.SharedPreferences
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.core.content.edit
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import io.github.vivekg7.dhun.data.Api
import io.github.vivekg7.dhun.data.Catalog
import io.github.vivekg7.dhun.data.Db
import io.github.vivekg7.dhun.data.Downloads
import io.github.vivekg7.dhun.data.LocalSongs
import io.github.vivekg7.dhun.data.Lyrics
import io.github.vivekg7.dhun.data.SongCache
import io.github.vivekg7.dhun.data.Store
import io.github.vivekg7.dhun.data.Sync
import io.github.vivekg7.dhun.play.Intents
import io.github.vivekg7.dhun.play.Playback
import io.github.vivekg7.dhun.play.Tempo
import io.github.vivekg7.dhun.ui.MiniPlayerStyle
import io.github.vivekg7.dhun.ui.theme.Palette
import io.github.vivekg7.dhun.ui.theme.ThemeMode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

/** Every long-lived object, built once (docs/plans/007_client_architecture.md: no DI framework). */
class App : Application() {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    val prefs by lazy { Prefs(this) }
    val db by lazy { Db.open(this) }
    val api by lazy { Api(prefs) }
    val store by lazy { Store(this) }
    val sync by lazy { Sync(this) }
    val playback by lazy { Playback(this) }
    val downloads by lazy { Downloads(this) }
    val lyrics by lazy { Lyrics(this) }
    val cache by lazy { SongCache(this) }
    val local by lazy { LocalSongs(this) }
    val intents by lazy { Intents(this) }

    /**
     * The catalogue in memory: about 30,000 songs today, built for 100,000,
     * browse and search faster there than through SQL
     * (docs/plans/036_fast_at_100k.md). The NAS songs and the phone's, once
     * both are read. Only a change rebuilds it: Room re-emits the table on
     * any write to it, and the phone's media index on any change at all.
     * The groupings and the search index are warmed on this scope right
     * after, so the main thread only ever reads them.
     */
    val catalog by lazy {
        combine(db.dao().songs().distinctUntilChanged(), local.songs.filterNotNull().distinctUntilChanged()) { nas, phone ->
            val t0 = System.nanoTime()
            val c = Catalog(nas + phone)
            Log.i(TAG, "catalogue: ${c.songs.size} songs built in ${(System.nanoTime() - t0) / 1_000_000} ms")
            c
        }.onEach { c ->
            scope.launch {
                val t0 = System.nanoTime()
                c.warm()
                Log.i(TAG, "catalogue: groupings and search index warm in ${(System.nanoTime() - t0) / 1_000_000} ms")
            }
        }.stateIn(scope, SharingStarted.Eagerly, Catalog(emptyList()))
    }

    override fun onCreate() {
        super.onCreate()
        app = this
        // Until 0.4.0, covers were kept in an HTTP cache; they are files now (Covers).
        scope.launch(Dispatchers.IO) { File(cacheDir, "http").deleteRecursively() }
        ProcessLifecycleOwner.get().lifecycle.addObserver(
            object : DefaultLifecycleObserver {
                override fun onStart(owner: LifecycleOwner) {
                    if (prefs.token.isEmpty()) return
                    scope.launch {
                        // Before the sync: its now-playing is judged by this device's id.
                        sync.checkHandoff()
                        sync.now()
                    }
                    // Downloads that could not start from the background go on now.
                    downloads.poke()
                }
            },
        )
    }

    /** Forgets the account, everything synced from it, and what was playing; the downloads only if not [keepDownloads]. */
    fun signOut(keepDownloads: Boolean) {
        scope.launch { forget(keepDownloads) }
    }

    /**
     * What the phone keeps is its own (docs/plans/026_without_an_account.md):
     * the phone songs' data, the queues made only of them, and, when kept,
     * the downloads with their songs, which stay playable without a server.
     */
    suspend fun forget(keepDownloads: Boolean) =
        withContext(Dispatchers.Main) {
            playback.reset()
            val who = prefs.userName
            prefs.signOut()
            // Who left what stays: the next sign-in, if someone else's, takes it over (SignInScreen).
            if (prefs.keptFrom.isEmpty()) prefs.keptFrom = who
            if (!keepDownloads) downloads.deleteAll()
            cache.deleteAll()
            val kept = db.dao().forgetAccount(keepDownloads)
            // The outbox is gone: the next account learns of the kept queues from these.
            for (q in kept) store.createQueue(q)
        }

    companion object {
        /** The logcat tag of the timing lines the speed targets are checked against (plan 036). */
        const val TAG = "Dhun"

        lateinit var app: App
            private set
    }
}

/**
 * Settings that belong to this device and are never synced: the sign-in and
 * the look. The ones that follow the user live in the database ([Store]).
 * Compose state, so a change redraws whatever reads it.
 */
class Prefs(
    context: Context,
) {
    private val sp = context.getSharedPreferences("dhun", Context.MODE_PRIVATE)

    var server by stored("server", "")
    var token by stored("token", "")
    var userName by stored("user", "")

    /** Chosen on the sign-in screen: the app opens with no account (docs/plans/026_without_an_account.md). */
    var withoutAccount by stored("withoutAccount", false)

    /**
     * The account whose sign-out left this phone's downloads and queues,
     * until the next sign-in's first sync. While set, no downloaded file is
     * deleted: the pins that cover them name playlists and marks that come
     * back only with that sync.
     */
    var keptFrom by stored("keptFrom", "")

    /** Whether the user is the admin, who manages family members (docs/plans/023_users_on_android.md). */
    var admin by stored("admin", false)

    /** False on a phone signed in before the flag was kept, until the server is asked. */
    val adminKnown get() = sp.contains("admin")

    /** The server's release, from its Dhun-Version header; empty until the first answer. */
    var serverVersion by stored("serverVersion", "")
    var libraryVersion by storedLong("libraryVersion")

    /** The song fields this phone has, so a new one ([LIBRARY_FORMAT]) fetches every song once. */
    var libraryFormat by storedLong("libraryFormat")
    var syncVersion by storedLong("syncVersion")

    /** This device as the server knows it, to tell its own playback from another device's (docs/plans/017_handoff.md). */
    var deviceId by storedLong("deviceId")

    /** When this device last reported its playback; a hand-off is offered only for something newer. */
    var stateAt by storedLong("stateAt")

    /** The `at` of the hand-off last dismissed, so ✕ is not undone by the next sync. */
    var handoffDismissed by stored("handoffDismissed", "")
    var activeQueue by stored("activeQueue", "")
    var themeMode by stored("theme", THEME)
    var palette by stored("palette", PALETTE)

    /**
     * The Search tab's last searches, newest first (docs/plans/029_search.md).
     * A search is kept when something it found is tapped, so half-typed
     * words are not. This phone's, like everything else in the box.
     */
    var recentSearches by mutableStateOf(sp.getString("recentSearches", "")!!.lines().filter { it.isNotBlank() })
        private set

    fun keepSearch(query: String) {
        val q = query.trim()
        if (q.isNotEmpty()) saveSearches(listOf(q) + recentSearches.filterNot { it.equals(q, ignoreCase = true) })
    }

    /** Forgets [query], or every search when null. */
    fun forgetSearch(query: String?) = saveSearches(if (query == null) emptyList() else recentSearches - query)

    private fun saveSearches(list: List<String>) {
        recentSearches = list.take(10)
        sp.edit { putString("recentSearches", recentSearches.joinToString("\n")) }
    }

    /** Which mini player shows over the tabs (docs/plans/022_mini_player_styles.md). */
    var miniPlayer by stored("miniPlayer", MINI_PLAYER)

    /**
     * Where the floating mini player was left, as fractions of the room it
     * has. At first on the right, a little up: at the very bottom it would
     * cover every tab's search box.
     */
    var floatingAt: Offset
        get() = Offset(sp.getFloat("floatingX", 1f), sp.getFloat("floatingY", 0.8f))
        set(o) = sp.edit { putFloat("floatingX", o.x).putFloat("floatingY", o.y) }

    /** Downloads (docs/plans/012_downloads.md): this phone's storage and network, so not synced. 0 is no limit. */
    var downloadLimitGb by stored("downloadLimitGb", DOWNLOAD_LIMIT_GB)
    var wifiOnly by stored("wifiOnly", WIFI_ONLY)

    /** The song cache's limit (docs/plans/019_networking_and_caching.md); 0 turns it off. */
    var cacheLimitGb by stored("cacheLimitGb", CACHE_LIMIT_GB)

    /**
     * Play speed and pitch for every song without its own
     * (docs/plans/016_speed_and_pitch.md). This device's only, by the owner's
     * choice: a song's own speed follows the user, the everyday one does not.
     */
    var tempo: Tempo
        get() = Tempo(sp.getFloat("speed", 1f), sp.getInt("semitones", 0))
        set(t) = sp.edit { putFloat("speed", t.speed).putInt("semitones", t.semitones) }

    /**
     * Songs already on the phone (docs/plans/025_phone_local_songs.md): off
     * until turned on, which asks for the permission. Files shorter than
     * [phoneMinSeconds], and those under an excluded folder, are left out.
     */
    var phoneSongs by stored("phoneSongs", PHONE_SONGS)
    var phoneMinSeconds by stored("phoneMinSeconds", PHONE_MIN_SECONDS)
    var phoneExcluded by stored("phoneExcluded", emptySet<String>())

    /** The sleep timer by the clock lets the song finish rather than fade out on time; this phone's, off at first. */
    var sleepFinishSong: Boolean
        get() = sp.getBoolean("sleepFinishSong", false)
        set(v) = sp.edit { putBoolean("sleepFinishSong", v) }

    /** Asked once, with the first download: Android 13 hides the progress notification without it. */
    var askedNotifications: Boolean
        get() = sp.getBoolean("askedNotifications", false)
        set(v) = sp.edit { putBoolean("askedNotifications", v) }

    /**
     * What each queue was started from ("album:…", "search"), for the listen
     * log (plan 008). The server has no field for it, so it stays here.
     */
    fun queueSource(queue: String) = sp.getString("source.$queue", "") ?: ""

    fun setQueueSource(
        queue: String,
        source: String,
    ) = sp.edit { putString("source.$queue", source) }

    /** The open listen, kept here so a killed app can close it on the next start (plan 008). */
    var openListen: String
        get() = sp.getString("openListen", "") ?: ""
        set(v) = sp.edit { putString("openListen", v) }

    /**
     * Reset to defaults, one settings page at a time
     * (docs/plans/034_settings_text_defaults_reset.md). The keys are then
     * removed rather than left set, as a synced setting is unset.
     */
    fun resetAppearance() {
        themeMode = THEME
        palette = PALETTE
        miniPlayer = MINI_PLAYER
        sp.edit { remove("theme").remove("palette").remove("miniPlayer") }
    }

    fun resetPhoneSongs() {
        phoneSongs = PHONE_SONGS
        phoneMinSeconds = PHONE_MIN_SECONDS
        sp.edit { remove("phoneSongs").remove("phoneMinSeconds") }
    }

    fun resetDownloads() {
        downloadLimitGb = DOWNLOAD_LIMIT_GB
        wifiOnly = WIFI_ONLY
        cacheLimitGb = CACHE_LIMIT_GB
        sp.edit { remove("downloadLimitGb").remove("wifiOnly").remove("cacheLimitGb") }
    }

    /** Forgets the account and the sync cursors; the look stays. */
    fun signOut() {
        token = ""
        userName = ""
        admin = false
        sp.edit { remove("admin") }
        serverVersion = ""
        activeQueue = ""
        libraryVersion = 0
        libraryFormat = 0
        syncVersion = 0
        openListen = ""
        deviceId = 0
        stateAt = 0
        handoffDismissed = ""
    }

    /** A value kept under [key], held as Compose state too. */
    private inner class Stored<T>(
        private val key: String,
        read: SharedPreferences.(String) -> T,
        private val write: SharedPreferences.Editor.(String, T) -> Unit,
    ) {
        private var state by mutableStateOf(sp.read(key))

        operator fun getValue(
            thisRef: Any?,
            p: Any?,
        ) = state

        operator fun setValue(
            thisRef: Any?,
            p: Any?,
            v: T,
        ) {
            state = v
            sp.edit { write(key, v) }
        }
    }

    private fun stored(
        key: String,
        def: String,
    ) = Stored(key, { getString(it, def) ?: def }) { k, v -> putString(k, v) }

    private fun stored(
        key: String,
        def: Boolean,
    ) = Stored(key, { getBoolean(it, def) }) { k, v -> putBoolean(k, v) }

    private fun stored(
        key: String,
        def: Int,
    ) = Stored(key, { getInt(it, def) }) { k, v -> putInt(k, v) }

    private fun stored(
        key: String,
        def: Set<String>,
    ) = Stored(key, { getStringSet(it, null)?.toSet() ?: def }) { k, v -> putStringSet(k, v) }

    /** By name: one no longer in the enum reads as [def]. */
    private inline fun <reified E : Enum<E>> stored(
        key: String,
        def: E,
    ) = Stored(key, { k -> enumValues<E>().firstOrNull { it.name == getString(k, null) } ?: def }) { k, v -> putString(k, v.name) }

    private fun storedLong(key: String) =
        object {
            operator fun getValue(
                thisRef: Any?,
                p: Any?,
            ) = sp.getLong(key, 0)

            operator fun setValue(
                thisRef: Any?,
                p: Any?,
                v: Long,
            ) = sp.edit { putLong(key, v) }
        }

    /** This device's defaults, which Settings marks and Reset to defaults goes back to. */
    companion object {
        val THEME = ThemeMode.System
        val PALETTE = Palette.DullOrange
        val MINI_PLAYER = MiniPlayerStyle.Bar
        const val DOWNLOAD_LIMIT_GB = 10
        const val WIFI_ONLY = true
        const val CACHE_LIMIT_GB = 3
        const val PHONE_SONGS = false
        const val PHONE_MIN_SECONDS = 30
    }
}
