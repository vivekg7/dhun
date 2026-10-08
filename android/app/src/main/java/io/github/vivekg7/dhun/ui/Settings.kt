package io.github.vivekg7.dhun.ui

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.BuildConfig
import io.github.vivekg7.dhun.data.ApiException
import io.github.vivekg7.dhun.data.bool
import io.github.vivekg7.dhun.data.bytes
import io.github.vivekg7.dhun.data.int
import io.github.vivekg7.dhun.data.string
import io.github.vivekg7.dhun.ui.theme.Palette
import io.github.vivekg7.dhun.ui.theme.ThemeMode
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonPrimitive

/**
 * Musicolet's shape (docs/plans/018_settings_layout.md): a short list of
 * categories, each its own screen of one-line rows. A choice shows its value
 * and opens a dialog; a setting that follows the user says so.
 */
enum class SettingsPage(
    val title: String,
) {
    Main("Settings"),
    Appearance("Appearance"),
    Playback("Playback"),
    Downloads("Downloads"),
    Account("Account"),
    About("About"),
}

@Composable
fun SettingsScreen(
    page: SettingsPage,
    open: (SettingsPage) -> Unit,
    onBack: () -> Unit,
) {
    key(page) {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
            Row(Modifier.padding(start = 4.dp, top = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Tip("Back") { IconButton(onBack) { Icon(Icons.Back, "Back") } }
                Text(page.title, style = MaterialTheme.typography.headlineSmall)
            }
            when (page) {
                SettingsPage.Main -> MainSettings(open)
                SettingsPage.Appearance -> AppearanceSettings()
                SettingsPage.Playback -> PlaybackSettings()
                SettingsPage.Downloads -> DownloadSettings()
                SettingsPage.Account -> AccountSettings()
                SettingsPage.About -> AboutSettings()
            }
        }
    }
}

@Composable
private fun MainSettings(open: (SettingsPage) -> Unit) {
    val prefs = App.app.prefs
    SettingRow("Appearance", "${prefs.themeMode.label} · ${prefs.palette.label}", Icons.Palette) { open(SettingsPage.Appearance) }
    SettingRow("Playback", "Long files, Listen Later", Icons.Play) { open(SettingsPage.Playback) }
    SettingRow("Downloads", "${limitLabel(prefs.downloadLimitGb)} · ${if (prefs.wifiOnly) "Wi-Fi only" else "Any network"}", Icons.Download) {
        open(SettingsPage.Downloads)
    }
    SettingRow("Account", "${prefs.userName} on ${prefs.server.substringAfter("://")}", Icons.Artist) { open(SettingsPage.Account) }
    SettingRow("About", "Dhun ${BuildConfig.VERSION_NAME}", Icons.Info) { open(SettingsPage.About) }
}

@Composable
private fun AppearanceSettings() {
    val prefs = App.app.prefs
    ChoiceRow("Theme", ThemeMode.entries, prefs.themeMode, { it.label }) { prefs.chooseTheme(it) }
    ChoiceRow(
        "Mini player",
        MiniPlayerStyle.entries,
        prefs.miniPlayer,
        { it.label },
        note = "Shown on every tab but Now playing. Floating is dragged anywhere; its cover plays and pauses, and a long press opens Now playing.",
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
                        PaletteSwatch(p, p == prefs.palette) {
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
private fun PlaybackSettings() {
    val app = App.app
    val settings by app.store.settings.collectAsState(emptyMap())
    val scope = rememberCoroutineScope()

    fun set(
        name: String,
        value: JsonPrimitive,
    ) = scope.launch { app.store.setting(name, value) }

    SectionLabel("Long files")
    ChoiceRow(
        "Continue long files",
        listOf(5, 10, 15, 20, 30, 60),
        settings.int("longFiles.minMinutes", 15),
        { "$it minutes and longer" },
        note = "Audiobooks, podcasts and mixes this long continue where you left them.",
        synced = true,
    ) { set("longFiles.minMinutes", JsonPrimitive(it)) }
    ChoiceRow(
        "Playing one again",
        listOf("auto", "ask", "off"),
        settings.string("longFiles.resume", "auto"),
        {
            when (it) {
                "auto" -> "Continue where you left it"
                "ask" -> "Ask"
                else -> "Start over"
            }
        },
        synced = true,
    ) { set("longFiles.resume", JsonPrimitive(it)) }

    SectionLabel("Listen Later")
    val autoRemove = settings.bool("listenLater.autoRemove", true)
    SwitchRow("Remove what you finish", "On all your devices", autoRemove) { set("listenLater.autoRemove", JsonPrimitive(it)) }
    ChoiceRow(
        "Finished at",
        listOf(80, 90, 95, 100),
        settings.int("listenLater.finishedPercent", 90),
        { "$it% heard" },
        note = "A file counts as finished once heard this far.",
        synced = true,
        enabled = autoRemove,
    ) { set("listenLater.finishedPercent", JsonPrimitive(it)) }
}

// This phone's storage and network, so not synced (docs/plans/012_downloads.md).
@Composable
private fun DownloadSettings() {
    val app = App.app
    val prefs = app.prefs
    val status by app.downloads.status.collectAsState()
    SettingRow("Storage", "On ${app.downloads.location()} · ${bytes(status.usedBytes)} used")
    ChoiceRow(
        "Storage limit",
        listOf(2, 5, 10, 20, 50, 0),
        prefs.downloadLimitGb,
        ::limitLabel,
        note = "At the limit, downloads stop. Nothing is deleted to make room.",
    ) {
        prefs.chooseDownloadLimit(it)
        app.downloads.poke()
    }
    SwitchRow("Wi-Fi only", if (prefs.wifiOnly) "Downloads wait for Wi-Fi" else "Downloads use mobile data too", prefs.wifiOnly) {
        prefs.chooseWifiOnly(it)
        app.downloads.poke()
    }
    val cached by app.cache.used.collectAsState()
    ChoiceRow(
        "Song cache",
        listOf(1, 3, 5, 10, 0),
        prefs.cacheLimitGb,
        { if (it == 0) "Off" else "$it GB" },
        note =
            "Songs you play, and the next ones in the queue (2 on mobile data, 10 on Wi-Fi), are kept " +
                "so they play without waiting. At the limit, the songs played longest ago make room.",
        summary = if (prefs.cacheLimitGb == 0) "Off" else "${prefs.cacheLimitGb} GB · ${bytes(cached)} used",
    ) {
        prefs.chooseCacheLimit(it)
        app.cache.poke()
    }
}

@Composable
private fun AccountSettings() {
    val app = App.app
    val error by app.sync.error.collectAsState()
    SettingRow("Signed in as ${app.prefs.userName}", app.prefs.server)
    error?.let { SettingRow("Last sync failed", it) }
    var confirm by remember { mutableStateOf(false) }
    SettingRow("Sign out", "This phone's downloads are deleted") { confirm = true }
    if (confirm) {
        AlertDialog(
            onDismissRequest = { confirm = false },
            title = { Text("Sign out?") },
            text = { Text("This phone's downloads are deleted. Your queues, playlists and history stay on the server.") },
            confirmButton = {
                TextButton({
                    confirm = false
                    app.signOut()
                }) { Text("Sign out") }
            },
            dismissButton = { TextButton({ confirm = false }) { Text("Cancel") } },
        )
    }
}

@Composable
private fun AboutSettings() {
    val prefs = App.app.prefs
    SettingRow("Dhun", "Version ${BuildConfig.VERSION_NAME}")
    if (prefs.serverVersion.isNotEmpty()) SettingRow("Server", "Version ${prefs.serverVersion}")
    SettingRow("Licence", "GPL-3.0")
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

/** A setting with a few values: the row shows the one chosen, a dialog picks another. */
@Composable
private fun <T> ChoiceRow(
    title: String,
    options: List<T>,
    selected: T,
    label: (T) -> String,
    note: String? = null,
    synced: Boolean = false,
    enabled: Boolean = true,
    /** The row's line, when it says more than the value chosen. */
    summary: String? = null,
    onPick: (T) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    SettingRow(title, summary ?: (label(selected) + if (synced) " · on all your devices" else ""), enabled = enabled) { open = true }
    if (!open) return
    AlertDialog(
        onDismissRequest = { open = false },
        title = { Text(title) },
        text = {
            Column {
                note?.let { DialogNote(it) }
                for (o in options) {
                    DialogRow(label(o), selected = o == selected) {
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
    }
}

/** First start, or after signing out. */
@Composable
fun SignInScreen() {
    val app = App.app
    val scope = rememberCoroutineScope()
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
                app.prefs.server = url
                app.prefs.userName = login.user.name
                app.prefs.deviceId = login.deviceId
                app.prefs.token = login.token
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
        Text("Sign in to your family's music server.", color = MaterialTheme.colorScheme.onSurfaceVariant)
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
