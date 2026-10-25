package io.github.vivekg7.dhun.ui

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInParent
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.BuildConfig
import io.github.vivekg7.dhun.Prefs
import io.github.vivekg7.dhun.data.ApiException
import io.github.vivekg7.dhun.data.LocalSongs
import io.github.vivekg7.dhun.data.Member
import io.github.vivekg7.dhun.data.Synced
import io.github.vivekg7.dhun.data.bytes
import io.github.vivekg7.dhun.data.parseTime
import io.github.vivekg7.dhun.ui.theme.Palette
import io.github.vivekg7.dhun.ui.theme.ThemeMode
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive

/**
 * Musicolet's shape (docs/plans/018_settings_layout.md): a short list of
 * categories, each its own screen of one-line rows. A choice shows its value
 * and opens a dialog. The words are kept short; what a setting is for is on
 * How it works (docs/plans/034_settings_text_defaults_reset.md).
 */
enum class SettingsPage(
    val title: String,
) {
    Main("Settings"),
    Appearance("Appearance"),
    Playback("Playback"),
    Library("Songs on this phone"),
    Downloads("Downloads"),
    Account("Account"),
    Family("Family members"),
    About("About"),
    Guide("How it works"),
}

/** The pages Reset to defaults puts back: what it asks, and what it does. */
private fun SettingsPage.reset(): Pair<String, () -> Unit>? {
    val app = App.app
    return when (this) {
        SettingsPage.Appearance -> {
            "Theme, accent colour and mini player go back to their defaults." to app.prefs::resetAppearance
        }

        // Unset rather than set to today's default, so a later default reaches them.
        SettingsPage.Playback -> {
            "They follow you, so all your devices change." to {
                app.scope.launch { for (s in Synced.all) app.store.setting(s.name, JsonNull) }
            }
        }

        SettingsPage.Library -> {
            "Excluded folders stay as they are." to {
                app.prefs.resetPhoneSongs()
                app.local.refresh()
            }
        }

        SettingsPage.Downloads -> {
            "Nothing downloaded is deleted." to {
                app.prefs.resetDownloads()
                app.downloads.poke()
                app.cache.poke()
            }
        }

        else -> {
            null
        }
    }
}

@Composable
fun SettingsScreen(
    page: SettingsPage,
    open: (SettingsPage) -> Unit,
    onBack: () -> Unit,
    /** Opens How it works at a heading, or at the top. */
    openGuide: (String?) -> Unit,
    guideAt: String?,
) {
    key(page) {
        val scroll = rememberScrollState()
        Column(Modifier.fillMaxSize().verticalScroll(scroll)) {
            Row(Modifier.padding(start = 4.dp, top = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Tip("Back") { IconButton(onBack) { Icon(Icons.Back, "Back") } }
                Text(page.title, Modifier.weight(1f), style = MaterialTheme.typography.headlineSmall)
                page.reset()?.let { (note, reset) -> ResetMenu(page.title, note, reset) }
            }
            when (page) {
                SettingsPage.Main -> MainSettings(open, openGuide)
                SettingsPage.Appearance -> AppearanceSettings(openGuide)
                SettingsPage.Playback -> PlaybackSettings(openGuide)
                SettingsPage.Library -> LibrarySettings()
                SettingsPage.Downloads -> DownloadSettings(openGuide)
                SettingsPage.Account -> AccountSettings(open)
                SettingsPage.Family -> FamilySettings()
                SettingsPage.About -> AboutSettings()
                SettingsPage.Guide -> GuideSettings(scroll, guideAt)
            }
        }
    }
}

/** ⋮ with Reset to defaults, which asks first. */
@Composable
private fun ResetMenu(
    title: String,
    note: String,
    reset: () -> Unit,
) {
    var menu by remember { mutableStateOf(false) }
    var confirm by remember { mutableStateOf(false) }
    Box {
        Tip("More options") { IconButton({ menu = true }) { Icon(Icons.More, "More options") } }
        DropdownMenu(menu, { menu = false }) { MenuItem("Reset to defaults", { menu = false }) { confirm = true } }
    }
    if (confirm) {
        AlertDialog(
            onDismissRequest = { confirm = false },
            title = { Text("Reset $title?") },
            text = { Text(note) },
            confirmButton = {
                TextButton({
                    confirm = false
                    reset()
                }) { Text("Reset") }
            },
            dismissButton = { TextButton({ confirm = false }) { Text("Cancel") } },
        )
    }
}

@Composable
private fun MainSettings(
    open: (SettingsPage) -> Unit,
    openGuide: (String?) -> Unit,
) {
    val prefs = App.app.prefs
    SettingRow("Appearance", "${prefs.themeMode.label} · ${prefs.palette.label}", Icons.Palette) { open(SettingsPage.Appearance) }
    SettingRow("Playback", "Long files, Listen Later", Icons.Play) { open(SettingsPage.Playback) }
    SettingRow("Songs on this phone", if (prefs.phoneSongs) "On" else "Off", Icons.Phone) { open(SettingsPage.Library) }
    SettingRow("Downloads", "${limitLabel(prefs.downloadLimitGb)} · ${if (prefs.wifiOnly) "Wi-Fi only" else "Any network"}", Icons.Download) {
        open(SettingsPage.Downloads)
    }
    val account = if (prefs.token.isEmpty()) "Not signed in" else "${prefs.userName} on ${prefs.server.substringAfter("://")}"
    SettingRow("Account", account, Icons.Artist) { open(SettingsPage.Account) }
    SettingRow("About", "Dhun ${BuildConfig.VERSION_NAME}", Icons.Info) { open(SettingsPage.About) }
    SettingRow("How it works", "Queues, syncing and every setting", Icons.Help) { openGuide(null) }
}

@Composable
private fun AppearanceSettings(openGuide: (String?) -> Unit) {
    val prefs = App.app.prefs
    ChoiceRow("Theme", ThemeMode.entries, prefs.themeMode, { it.label }, default = Prefs.THEME) { prefs.chooseTheme(it) }
    ChoiceRow(
        "Mini player",
        MiniPlayerStyle.entries,
        prefs.miniPlayer,
        { it.label },
        default = Prefs.MINI_PLAYER,
        guide = { openGuide("Mini player") },
    ) { prefs.chooseMiniPlayer(it) }
    var palette by remember { mutableStateOf(false) }
    SettingRow("Accent colour", prefs.palette.label) { palette = true }
    if (palette) {
        AlertDialog(
            onDismissRequest = { palette = false },
            title = { Text("Accent colour") },
            text = {
                FlowRow(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    for (p in Palette.entries) {
                        PaletteSwatch(p, p == prefs.palette, p == Prefs.PALETTE) {
                            prefs.choosePalette(p)
                            palette = false
                        }
                    }
                }
            },
            confirmButton = { TextButton({ palette = false }) { Text("Cancel") } },
        )
    }
}

// Synced: these follow the user to every device (docs/plans/009_resume_long_files.md).
@Composable
private fun PlaybackSettings(openGuide: (String?) -> Unit) {
    val app = App.app
    val settings by app.store.settings.collectAsState()
    val scope = rememberCoroutineScope()

    @Composable
    fun <T> Choice(
        title: String,
        s: Synced<T>,
        label: (T) -> String,
        note: String? = null,
        guide: String? = null,
        enabled: Boolean = true,
    ) = ChoiceRow(title, s.options, s.of(settings), label, note, s.default, enabled, guide = guide?.let { { openGuide(it) } }) {
        scope.launch { app.store.setting(s.name, s.json(it)) }
    }

    Text(
        "These follow you to all your devices.",
        Modifier.padding(horizontal = 20.dp, vertical = 4.dp),
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    SectionLabel("Buttons")
    Choice("Previous", Synced.previousRestart, { if (it) "Restart song first" else "Previous song" })

    SectionLabel("Long files")
    Choice("Remember position", Synced.longMinMinutes, { "$it min and longer" }, "For audiobooks, podcasts and mixes.", guide = "Remember position")
    Choice("When played again", Synced.longResume, {
        when (it) {
            "auto" -> "Resume"
            "ask" -> "Ask"
            else -> "Start over"
        }
    })
    Choice(
        "Previous button",
        Synced.longPrevious,
        {
            when (it) {
                "keep" -> "Previous file"
                "seek" -> "Seek back (headset)"
                "ignore" -> "Ignore headset"
                else -> "Same as songs"
            }
        },
        "On the screen, Previous always goes to the previous file.",
        guide = "Previous button",
    )
    Choice("Next button", Synced.longNext, { if (it == "seek") "Seek forward (headset)" else "Next file" }, guide = "Next button")
    Choice("Seek back", Synced.seekBack, { "$it s" })
    Choice("Seek forward", Synced.seekForward, { "$it s" })
    val rewind = Synced.rewind.of(settings)
    Choice("Rewind on resume", Synced.rewind, { if (it == 0) "Off" else "$it s" })
    Choice("After pauses of", Synced.rewindAfter, { "$it min" }, enabled = rewind > 0)
    Choice(
        "Cache ahead",
        Synced.longCacheAhead,
        { if (it == 0) "Whole file" else "$it min" },
        "A long file is cached around where you are, not whole.",
        guide = "Song cache",
    )

    SectionLabel("Listen Later")
    val autoRemove = Synced.laterAutoRemove.of(settings)
    SwitchRow("Remove when finished", null, autoRemove) { scope.launch { app.store.setting(Synced.laterAutoRemove.name, JsonPrimitive(it)) } }
    Choice("Finished at", Synced.laterFinishedPercent, { "$it%" }, enabled = autoRemove)
}

// Songs already on the phone (docs/plans/025_phone_local_songs.md): this phone's, so not synced.
@Composable
private fun LibrarySettings() {
    val app = App.app
    val prefs = app.prefs
    val context = LocalContext.current
    val phone by app.local.songs.collectAsState()
    val ask =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            if (granted) {
                prefs.choosePhoneSongs(true)
                app.local.refresh()
            } else {
                android.widget.Toast
                    .makeText(context, "Without access to music, Dhun cannot show the phone's songs", android.widget.Toast.LENGTH_LONG)
                    .show()
            }
        }
    val allowed = app.local.allowed()
    val summary =
        when {
            !prefs.phoneSongs -> null
            !allowed -> "Tap to allow access to music"
            else -> "${phone?.size ?: 0} songs"
        }
    SwitchRow("Songs on this phone", summary, prefs.phoneSongs && allowed) { on ->
        if (on && !allowed) {
            ask.launch(LocalSongs.PERMISSION)
        } else {
            prefs.choosePhoneSongs(on)
            app.local.refresh()
        }
    }
    ChoiceRow(
        "Skip files shorter than",
        listOf(0, 10, 30, 60, 120),
        prefs.phoneMinSeconds,
        { if (it == 0) "Off" else "$it s" },
        default = Prefs.PHONE_MIN_SECONDS,
        enabled = prefs.phoneSongs,
    ) {
        prefs.choosePhoneMinSeconds(it)
        app.local.refresh()
    }
    var excluding by remember { mutableStateOf(false) }
    val excluded = prefs.phoneExcluded
    SettingRow(
        "Excluded folders",
        if (excluded.isEmpty()) "None" else excluded.joinToString(", ") { it.removePrefix(LocalSongs.PHONE_ROOT + "/") },
        enabled = prefs.phoneSongs,
    ) { excluding = true }
    if (excluding) ExcludedFoldersDialog { excluding = false }
}

/** Every folder of the phone holding audio, ticked to leave it and its subfolders out. */
@Composable
private fun ExcludedFoldersDialog(onDismiss: () -> Unit) {
    val app = App.app
    val folders by app.local.folders.collectAsState()
    var chosen by remember { mutableStateOf(app.prefs.phoneExcluded) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Excluded folders") },
        text = {
            LazyColumn(Modifier.heightIn(max = 420.dp)) {
                items(folders, key = { it.first }) { (path, count) ->
                    val on = path in chosen
                    Row(
                        Modifier.fillMaxWidth().clickable { chosen = if (on) chosen - path else chosen + path },
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Checkbox(on, null)
                        Spacer(Modifier.width(12.dp))
                        Text(
                            path.removePrefix(LocalSongs.PHONE_ROOT + "/").ifEmpty { "Top folder" },
                            Modifier.weight(1f),
                            style = MaterialTheme.typography.bodyMedium,
                        )
                        Text("$count", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
        },
        confirmButton = {
            TextButton({
                app.prefs.choosePhoneExcluded(chosen)
                app.local.refresh()
                onDismiss()
            }) { Text("Done") }
        },
        dismissButton = { TextButton(onDismiss) { Text("Cancel") } },
    )
}

// This phone's storage and network, so not synced (docs/plans/012_downloads.md).
@Composable
private fun DownloadSettings(openGuide: (String?) -> Unit) {
    val app = App.app
    val prefs = app.prefs
    val status by app.downloads.status.collectAsState()
    SettingRow("Storage", "On ${app.downloads.location()} · ${bytes(status.usedBytes)} used")
    ChoiceRow(
        "Storage limit",
        listOf(2, 5, 10, 20, 50, 0),
        prefs.downloadLimitGb,
        ::limitLabel,
        note = "At the limit, downloads stop. Nothing is deleted.",
        default = Prefs.DOWNLOAD_LIMIT_GB,
    ) {
        prefs.chooseDownloadLimit(it)
        app.downloads.poke()
    }
    SwitchRow("Wi-Fi only", null, prefs.wifiOnly) {
        prefs.chooseWifiOnly(it)
        app.downloads.poke()
    }
    val cached by app.cache.used.collectAsState()
    ChoiceRow(
        "Song cache",
        listOf(1, 3, 5, 10, 0),
        prefs.cacheLimitGb,
        { if (it == 0) "Off" else "$it GB" },
        default = Prefs.CACHE_LIMIT_GB,
        guide = { openGuide("Song cache") },
        summary = if (prefs.cacheLimitGb == 0) "Off" else "${prefs.cacheLimitGb} GB · ${bytes(cached)} used",
    ) {
        prefs.chooseCacheLimit(it)
        app.cache.poke()
    }
}

@Composable
private fun AccountSettings(open: (SettingsPage) -> Unit) {
    val app = App.app
    val prefs = app.prefs
    val error by app.sync.error.collectAsState()
    if (prefs.token.isEmpty()) {
        // Back to the sign-in screen, which can also return here (docs/plans/026_without_an_account.md).
        SettingRow("Sign in", "To your family's music server") { prefs.withoutAccount = false }
    } else {
        SettingRow("Signed in as ${prefs.userName}", prefs.server)
        error?.let { SettingRow("Last sync failed", it) }
        if (prefs.admin) SettingRow("Family members", "Add a member, reset a forgotten password") { open(SettingsPage.Family) }
    }
    // Revoked, the account is still this phone's until signed out.
    if (prefs.userName.isEmpty()) return
    var confirm by remember { mutableStateOf(false) }
    val files by app.downloads.files.collectAsState()
    SettingRow("Sign out", "Keep or delete this phone's downloads") { confirm = true }
    if (confirm) {
        val out = { keep: Boolean ->
            confirm = false
            app.signOut(keepDownloads = keep)
        }
        AlertDialog(
            onDismissRequest = { confirm = false },
            title = { Text("Sign out?") },
            text = {
                val size = bytes(files.values.sumOf { it.size })
                val kept = if (files.isEmpty()) "" else "\n\nKept, this phone's ${files.size} downloaded songs ($size) still play without signing in."
                Text("Your queues, playlists and history stay on the server, and the songs on this phone keep their favourites and counts here.$kept")
            },
            confirmButton = {
                if (files.isEmpty()) {
                    TextButton({ out(true) }) { Text("Sign out") }
                } else {
                    Row {
                        TextButton({ out(false) }) { Text("Delete downloads") }
                        TextButton({ out(true) }) { Text("Keep downloads") }
                    }
                }
            },
            dismissButton = { TextButton({ confirm = false }) { Text("Cancel") } },
        )
    }
}

/**
 * The admin's list of users (docs/plans/023_users_on_android.md). Asked of the
 * server each time it opens, and online only: these are actions on the
 * server, not part of anyone's synced data.
 */
@Composable
private fun FamilySettings() {
    val app = App.app
    val scope = rememberCoroutineScope()
    var members by remember { mutableStateOf<List<Member>?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var adding by remember { mutableStateOf(false) }
    var resetting by remember { mutableStateOf<Member?>(null) }

    fun load() {
        error = null
        scope.launch {
            try {
                members = app.api.members()
            } catch (e: ApiException) {
                error = e.message
            } catch (e: java.io.IOException) {
                error = "This needs the server, which cannot be reached now."
            } catch (e: kotlinx.serialization.SerializationException) {
                error = "The server's answer could not be read."
            }
        }
    }
    LaunchedEffect(Unit) { load() }

    error?.let { SettingRow("Could not load family members", "${it.trimEnd('.')}. Tap to try again.") { load() } }
    members?.forEach { m ->
        if (m.admin) {
            // Resetting it here would sign out this phone.
            SettingRow(m.name, "Admin · its password is changed on the NAS")
        } else {
            SettingRow(m.name, memberSummary(m)) { resetting = m }
        }
    }
    if (members != null) SettingRow("Add a member") { adding = true }

    if (adding) {
        MemberDialog("Add a member", null, "Add", askName = true, onDismiss = { adding = false }) { name, password ->
            app.api.addMember(name, password)
            adding = false
            load()
        }
    }
    resetting?.let { m ->
        MemberDialog(
            "Reset ${m.name}'s password",
            "Every device of theirs is signed out until they sign in with the new one. Nothing on their phone is lost.",
            "Reset",
            askName = false,
            onDismiss = { resetting = null },
        ) { _, password ->
            app.api.resetPassword(m.id, password)
            resetting = null
            load()
        }
    }
}

private fun memberSummary(m: Member): String {
    val seen = parseTime(m.lastSeenAt)
    return when {
        m.devices == 0 -> {
            "Not signed in on any device"
        }

        seen == 0L -> {
            "Signed in on ${m.devices} device${if (m.devices == 1) "" else "s"}"
        }

        else -> {
            val ago =
                if (System.currentTimeMillis() - seen < 60_000) {
                    "just now"
                } else {
                    android.text.format.DateUtils
                        .getRelativeTimeSpanString(seen)
                }
            "Signed in on ${m.devices} device${if (m.devices == 1) "" else "s"} · seen $ago"
        }
    }.let { "$it · tap to reset the password" }
}

/**
 * A name (when adding) and a password. The server's rules for both show as
 * its own error, rather than copied here where they could drift from it.
 */
@Composable
private fun MemberDialog(
    title: String,
    note: String?,
    confirm: String,
    askName: Boolean,
    onDismiss: () -> Unit,
    submit: suspend (name: String, password: String) -> Unit,
) {
    val scope = rememberCoroutineScope()
    var name by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                note?.let { DialogNote(it) }
                if (askName) OutlinedTextField(name, { name = it }, Modifier.fillMaxWidth(), label = { Text("Name") }, singleLine = true)
                // Shown as typed: the admin reads it out to the member, and a typo
                // in a hidden password would lock them out.
                OutlinedTextField(
                    password,
                    { password = it },
                    Modifier.fillMaxWidth(),
                    label = { Text("Password") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                )
                error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            }
        },
        confirmButton = {
            TextButton({
                busy = true
                error = null
                scope.launch {
                    try {
                        submit(name.trim(), password)
                    } catch (e: ApiException) {
                        error = e.message
                    } catch (e: java.io.IOException) {
                        error = "Could not reach the server."
                    } catch (e: kotlinx.serialization.SerializationException) {
                        error = "The server's answer could not be read."
                    } finally {
                        busy = false
                    }
                }
            }, enabled = !busy && password.isNotEmpty() && (!askName || name.isNotBlank())) { Text(confirm) }
        },
        dismissButton = { TextButton(onDismiss) { Text("Cancel") } },
    )
}

@Composable
private fun AboutSettings() {
    val prefs = App.app.prefs
    SettingRow("Dhun", "Version ${BuildConfig.VERSION_NAME}")
    if (prefs.serverVersion.isNotEmpty()) SettingRow("Server", "Version ${prefs.serverVersion}")
    SettingRow("Licence", "GPL-3.0")
}

/**
 * How it works: docs/GUIDE.md, copied into the assets by scripts/guide.sh.
 * Headings, paragraphs, "- " items and **bold** are all it uses. Opened at
 * [at], a heading's text, when an ⓘ asked for one.
 */
@Composable
private fun GuideSettings(
    scroll: ScrollState,
    at: String?,
) {
    val context = LocalContext.current
    val blocks =
        remember {
            guideBlocks(
                context.assets
                    .open("guide.md")
                    .bufferedReader()
                    .use { it.readText() },
            )
        }
    var target by remember { mutableStateOf<Int?>(null) }
    LaunchedEffect(target) { target?.let { scroll.scrollTo(it) } }
    val c = MaterialTheme.colorScheme
    Column(Modifier.padding(start = 20.dp, end = 20.dp, bottom = 24.dp)) {
        for ((level, text) in blocks) {
            when (level) {
                2 -> {
                    Text(text, Modifier.padding(top = 20.dp, bottom = 4.dp), style = MaterialTheme.typography.titleMedium, color = c.primary)
                }

                3 -> {
                    Text(
                        text,
                        Modifier.padding(top = 12.dp, bottom = 2.dp).then(
                            if (text == at) Modifier.onGloballyPositioned { target = it.positionInParent().y.toInt() } else Modifier,
                        ),
                        style = MaterialTheme.typography.titleSmall,
                    )
                }

                0 -> {
                    Text(bold(text), Modifier.padding(vertical = 4.dp), style = MaterialTheme.typography.bodyMedium)
                }

                else -> {
                    Row(Modifier.padding(vertical = 2.dp)) {
                        Text("•", Modifier.width(16.dp), style = MaterialTheme.typography.bodyMedium)
                        Text(bold(text), style = MaterialTheme.typography.bodyMedium)
                    }
                }
            }
        }
    }
}

/** The guide as (level, text): 2 and 3 for headings, 0 for a paragraph, -1 for a list item. The title is the page's. */
internal fun guideBlocks(md: String): List<Pair<Int, String>> {
    val out = mutableListOf<Pair<Int, String>>()
    for (line in md.lines()) {
        val t = line.trim()
        when {
            t.isEmpty() || t.startsWith("# ") -> out += 0 to ""

            t.startsWith("### ") -> out += 3 to t.drop(4)

            t.startsWith("## ") -> out += 2 to t.drop(3)

            t.startsWith("- ") -> out += -1 to t.drop(2)

            // A line of a paragraph or item goes on with the one before it.
            out.lastOrNull()?.let { it.first <= 0 && it.second.isNotEmpty() } == true -> out[out.lastIndex] = out.last().let { it.first to "${it.second} $t" }

            else -> out += 0 to t
        }
    }
    return out.filter { it.second.isNotEmpty() }
}

private fun bold(text: String) =
    buildAnnotatedString {
        text.split("**").forEachIndexed { i, part -> if (i % 2 == 1) withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(part) } else append(part) }
    }

private fun limitLabel(gb: Int) = if (gb == 0) "No limit" else "$gb GB"

/** One setting: a title, what it is set to, and an icon on the list of categories only. */
@Composable
private fun SettingRow(
    title: String,
    summary: String? = null,
    icon: ImageVector? = null,
    enabled: Boolean = true,
    trailing: @Composable (() -> Unit)? = null,
    onClick: (() -> Unit)? = null,
) {
    val c = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .then(if (onClick != null) Modifier.clickable(enabled = enabled, onClick = onClick) else Modifier)
            .alpha(if (enabled) 1f else 0.38f)
            .padding(horizontal = 20.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(icon, null, Modifier.size(24.dp), tint = c.onSurfaceVariant)
            Spacer(Modifier.width(20.dp))
        }
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            summary?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = c.onSurfaceVariant) }
        }
        trailing?.let {
            Spacer(Modifier.width(16.dp))
            it()
        }
    }
}

@Composable
private fun SwitchRow(
    title: String,
    summary: String?,
    on: Boolean,
    onChange: (Boolean) -> Unit,
) = SettingRow(title, summary, trailing = { Switch(on, null) }) { onChange(!on) }

/**
 * A setting with a few values: the row shows the one chosen, a dialog picks
 * another, with the [default] marked. [guide] puts an ⓘ in the dialog's
 * title that opens How it works where it explains the setting.
 */
@Composable
private fun <T> ChoiceRow(
    title: String,
    options: List<T>,
    selected: T,
    label: (T) -> String,
    note: String? = null,
    default: T? = null,
    enabled: Boolean = true,
    /** The row's line, when it says more than the value chosen. */
    summary: String? = null,
    guide: (() -> Unit)? = null,
    onPick: (T) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    SettingRow(title, summary ?: label(selected), enabled = enabled) { open = true }
    if (!open) return
    AlertDialog(
        onDismissRequest = { open = false },
        title = {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(title, Modifier.weight(1f))
                if (guide != null) {
                    Tip("How it works") {
                        IconButton({
                            open = false
                            guide()
                        }) { Icon(Icons.InfoOutline, "How it works", tint = MaterialTheme.colorScheme.onSurfaceVariant) }
                    }
                }
            }
        },
        text = {
            Column {
                note?.let { DialogNote(it) }
                for (o in options) {
                    DialogRow(label(o), selected = o == selected, detail = if (o == default) "Default" else null) {
                        open = false
                        onPick(o)
                    }
                }
            }
        },
        confirmButton = { TextButton({ open = false }) { Text("Cancel") } },
    )
}

@Composable
private fun PaletteSwatch(
    p: Palette,
    selected: Boolean,
    default: Boolean,
    onClick: () -> Unit,
) {
    val c = MaterialTheme.colorScheme
    val accent = if (c.background.luminance() < 0.5f) p.dark else p.light
    Column(Modifier.width(88.dp).clip(RoundedCornerShape(12.dp)).clickable(onClick = onClick), horizontalAlignment = Alignment.CenterHorizontally) {
        Box(
            Modifier.size(60.dp).then(if (selected) Modifier.border(2.dp, accent, CircleShape) else Modifier),
            contentAlignment = Alignment.Center,
        ) {
            Box(Modifier.size(48.dp).background(accent, CircleShape), contentAlignment = Alignment.Center) {
                if (selected) Icon(Icons.Check, null, Modifier.size(22.dp), tint = androidx.compose.ui.graphics.Color.White)
            }
        }
        Text(p.label, Modifier.padding(top = 6.dp), style = MaterialTheme.typography.bodySmall, color = if (selected) c.onSurface else c.onSurfaceVariant)
        if (default) Text("Default", style = MaterialTheme.typography.labelSmall, color = c.onSurfaceVariant)
    }
}

/** First start, after signing out, or when the server has revoked this phone's token. */
@Composable
fun SignInScreen() {
    val app = App.app
    val scope = rememberCoroutineScope()
    // A revoked token keeps the name and everything synced; Sign out clears both.
    val revoked = remember { app.prefs.userName.isNotEmpty() }
    var server by remember { mutableStateOf(app.prefs.server) }
    var user by remember { mutableStateOf(app.prefs.userName) }
    var password by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }

    fun signIn() {
        val url = serverUrl(server)
        busy = true
        error = null
        scope.launch {
            try {
                val login = app.api.login(url, user.trim(), password, Build.MODEL)
                // Someone else's data stays theirs. Only the name is compared: the
                // same server is reached at its home or its Tailscale address.
                // The downloads stay, as on any sign-out that keeps them: the library is the family's.
                if (app.prefs.userName.isNotEmpty() && login.user.name != app.prefs.userName) app.forget(keepDownloads = true)
                // Left by someone else: the downloads and queues are theirs now (docs/plans/026_without_an_account.md).
                if (app.prefs.keptFrom.isNotEmpty() && app.prefs.keptFrom != login.user.name) {
                    app.downloads.adopt()
                    app.playback.reissueQueues()
                    app.prefs.keptFrom = login.user.name
                }
                app.prefs.server = url
                app.prefs.userName = login.user.name
                app.prefs.admin = login.user.admin
                app.prefs.deviceId = login.deviceId
                app.prefs.token = login.token
                app.prefs.withoutAccount = false
                // This screen leaves as soon as the token is set; the first sync outlives it.
                app.scope.launch { app.sync.now() }
            } catch (e: ApiException) {
                error = if (e.code == 401) "Wrong name or password" else e.message
            } catch (e: java.io.IOException) {
                error = "Could not reach $url (${e.message})"
            } catch (e: Exception) {
                error = e.message ?: e.toString()
            } finally {
                busy = false
            }
        }
    }

    // Android 17 lets an app reach the home network only with the user's
    // permission, and the NAS is on it. Asked here, where the reason is obvious.
    val askLocal = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { signIn() }
    val context = LocalContext.current
    val submit = {
        if (Build.VERSION.SDK_INT >= 37 && context.checkSelfPermission(Manifest.permission.ACCESS_LOCAL_NETWORK) != PackageManager.PERMISSION_GRANTED) {
            askLocal.launch(Manifest.permission.ACCESS_LOCAL_NETWORK)
        } else {
            signIn()
        }
    }

    // Without an account the app is the phone's songs: asked for here, and opened either way.
    val askMusic =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            if (granted) app.prefs.choosePhoneSongs(true)
            app.local.refresh()
            app.prefs.withoutAccount = true
        }
    val withoutAccount = {
        if (app.local.allowed() || app.prefs.phoneSongs) {
            app.prefs.withoutAccount = true
        } else {
            askMusic.launch(LocalSongs.PERMISSION)
        }
    }

    Column(
        Modifier
            .fillMaxSize()
            .imePadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 28.dp, vertical = 48.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Box(Modifier.size(72.dp).background(MaterialTheme.colorScheme.primary, CircleShape), contentAlignment = Alignment.Center) {
            Icon(
                androidx.compose.ui.res
                    .painterResource(io.github.vivekg7.dhun.R.drawable.ic_stat),
                null,
                Modifier.size(36.dp),
                tint = MaterialTheme.colorScheme.onPrimary,
            )
        }
        Text("Dhun", style = MaterialTheme.typography.displaySmall.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Bold))
        Text(
            if (revoked) {
                "Your password was changed or this phone was signed out. Sign in again; nothing on this phone is lost."
            } else {
                "Sign in to your family's music server."
            },
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(
            server,
            { server = it },
            Modifier.fillMaxWidth(),
            label = { Text("Server") },
            placeholder = { Text("my-nas:8585") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Next),
        )
        OutlinedTextField(user, {
            user = it
        }, Modifier.fillMaxWidth(), label = { Text("Name") }, singleLine = true, keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next))
        OutlinedTextField(
            password,
            { password = it },
            Modifier.fillMaxWidth(),
            label = { Text("Password") },
            singleLine = true,
            visualTransformation = PasswordVisualTransformation(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
        )
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        Button(submit, Modifier.fillMaxWidth().height(52.dp), enabled = !busy && server.isNotBlank() && user.isNotBlank() && password.isNotEmpty()) {
            Text(if (busy) "Signing in…" else "Sign in")
        }
        TextButton(withoutAccount, Modifier.fillMaxWidth(), enabled = !busy) { Text("Use without signing in") }
        val kept by app.downloads.files.collectAsState()
        Text(
            "Play the songs on this phone${if (kept.isEmpty()) "" else " and the downloads kept here"}. " +
                "Sign in any time from Settings → Account.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/**
 * What people type is "my-nas" or "192.168.1.50:8585": add the scheme
 * and the default port (docs/plans/003_deployment.md) when they are missing.
 */
fun serverUrl(input: String): String {
    var s = input.trim().trimEnd('/')
    if (!s.contains("://")) s = "http://$s"
    val host = s.substringAfter("://").substringBefore('/')
    if (!host.contains(':') && s.startsWith("http://")) s = s.replaceFirst(host, "$host:8585")
    return s
}
