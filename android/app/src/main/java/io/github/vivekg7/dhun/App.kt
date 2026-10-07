package io.github.vivekg7.dhun

import android.app.Application
import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.edit
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import io.github.vivekg7.dhun.data.Api
import io.github.vivekg7.dhun.data.Catalog
import io.github.vivekg7.dhun.data.Db
import io.github.vivekg7.dhun.data.Downloads
import io.github.vivekg7.dhun.data.Store
import io.github.vivekg7.dhun.data.Sync
import io.github.vivekg7.dhun.play.Playback
import io.github.vivekg7.dhun.ui.theme.Palette
import io.github.vivekg7.dhun.ui.theme.ThemeMode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** Every long-lived object, built once (docs/plans/007_client_architecture.md: no DI framework). */
class App : Application() {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    val prefs by lazy { Prefs(this) }
    val db by lazy { Db.open(this) }
    val api by lazy { Api(this, prefs) }
    val store by lazy { Store(this) }
    val sync by lazy { Sync(this) }
    val playback by lazy { Playback(this) }
    val downloads by lazy { Downloads(this) }

    /** The catalogue in memory: 7,000 songs browse and search faster there than through SQL. */
    val catalog by lazy {
        db
            .dao()
            .songs()
            .map { Catalog(it) }
            .stateIn(scope, SharingStarted.Eagerly, Catalog(emptyList()))
    }

    override fun onCreate() {
        super.onCreate()
        app = this
        ProcessLifecycleOwner.get().lifecycle.addObserver(
            object : DefaultLifecycleObserver {
                override fun onStart(owner: LifecycleOwner) {
                    if (prefs.token.isEmpty()) return
                    scope.launch { sync.now() }
                    // Downloads that could not start from the background go on now.
                    downloads.poke()
                }
            },
        )
    }

    /** Forgets the account, everything synced from it, what was playing, and its downloads. */
    fun signOut() {
        scope.launch(kotlinx.coroutines.Dispatchers.Main) {
            playback.reset()
            prefs.signOut()
            downloads.deleteAll()
            db.clearAllTables()
        }
    }

    companion object {
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
    var libraryVersion by storedLong("libraryVersion")
    var syncVersion by storedLong("syncVersion")
    var activeQueue by stored("activeQueue", "")
    var themeMode by mutableStateOf(enumOr(sp.getString("theme", null), ThemeMode.System))
        private set
    var palette by mutableStateOf(enumOr(sp.getString("palette", null), Palette.DullOrange))
        private set

    /** Downloads (docs/plans/012_downloads.md): this phone's storage and network, so not synced. 0 is no limit. */
    var downloadLimitGb by mutableIntStateOf(sp.getInt("downloadLimitGb", 10))
        private set
    var wifiOnly by mutableStateOf(sp.getBoolean("wifiOnly", true))
        private set

    fun chooseDownloadLimit(gb: Int) {
        downloadLimitGb = gb
        sp.edit { putInt("downloadLimitGb", gb) }
    }

    fun chooseWifiOnly(on: Boolean) {
        wifiOnly = on
        sp.edit { putBoolean("wifiOnly", on) }
    }

    /** Asked once, with the first download: Android 13 hides the progress notification without it. */
    var askedNotifications: Boolean
        get() = sp.getBoolean("askedNotifications", false)
        set(v) = sp.edit { putBoolean("askedNotifications", v) }

    fun chooseTheme(mode: ThemeMode) {
        themeMode = mode
        sp.edit { putString("theme", mode.name) }
    }

    fun choosePalette(p: Palette) {
        palette = p
        sp.edit { putString("palette", p.name) }
    }

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

    /** Forgets the account and the sync cursors; the look stays. */
    fun signOut() {
        token = ""
        userName = ""
        activeQueue = ""
        libraryVersion = 0
        syncVersion = 0
        openListen = ""
    }

    private fun stored(
        key: String,
        def: String,
    ) = StoredString(key, def)

    private inner class StoredString(
        val key: String,
        def: String,
    ) {
        private var state by mutableStateOf(sp.getString(key, def) ?: def)

        operator fun getValue(
            thisRef: Any?,
            p: Any?,
        ) = state

        operator fun setValue(
            thisRef: Any?,
            p: Any?,
            v: String,
        ) {
            state = v
            sp.edit { putString(key, v) }
        }
    }

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

    private inline fun <reified E : Enum<E>> enumOr(
        name: String?,
        def: E,
    ) = enumValues<E>().firstOrNull { it.name == name } ?: def
}
