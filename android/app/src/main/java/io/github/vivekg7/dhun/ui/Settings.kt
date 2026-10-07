package io.github.vivekg7.dhun.ui

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
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
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.luminance
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

@Composable
fun SettingsScreen(onBack: () -> Unit) {
    val app = App.app
    val prefs = app.prefs
    val settings by app.store.settings.collectAsState(emptyMap())
    val scope = rememberCoroutineScope()

    fun set(
        name: String,
        value: JsonPrimitive,
    ) = scope.launch { app.store.setting(name, value) }

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        Row(Modifier.padding(start = 4.dp, top = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onBack) { Icon(Icons.Back, "Back") }
            Text("Settings", style = MaterialTheme.typography.headlineSmall)
        }

        SectionLabel("Theme")
        Chips(ThemeMode.entries, prefs.themeMode, { it.label }) { prefs.chooseTheme(it) }

        SectionLabel("Colour")
        FlowRow(Modifier.padding(horizontal = 12.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            for (p in Palette.entries) PaletteSwatch(p, p == prefs.palette) { prefs.choosePalette(p) }
        }

        // Synced: these follow the user to every device (docs/plans/009_resume_long_files.md).
        SectionLabel("Long files")
        Hint("Audiobooks, podcasts and mixes this long continue where you left them, on any device.")
        Chips(listOf(5, 10, 15, 20, 30, 60), settings.int("longFiles.minMinutes", 15), { "$it min" }) { set("longFiles.minMinutes", JsonPrimitive(it)) }
        Chips(listOf("auto", "ask", "off"), settings.string("longFiles.resume", "auto"), {
            when (it) {
                "auto" -> "Continue"
                "ask" -> "Ask"
                else -> "Start over"
            }
        }) { set("longFiles.resume", JsonPrimitive(it)) }

        SectionLabel("Listen Later")
        val autoRemove = settings.bool("listenLater.autoRemove", true)
        Row(
            Modifier
                .fillMaxWidth()
                .clickable {
                    set("listenLater.autoRemove", JsonPrimitive(!autoRemove))
                }.padding(horizontal = 20.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Remove what you finish", Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge)
            Switch(autoRemove, { set("listenLater.autoRemove", JsonPrimitive(it)) })
        }
        if (autoRemove) {
            Hint("Finished means heard this far into the file.")
            Chips(
                listOf(80, 90, 95, 100),
                settings.int("listenLater.finishedPercent", 90),
                { "$it%" },
            ) { set("listenLater.finishedPercent", JsonPrimitive(it)) }
        }

        // This phone's storage and network, so not synced (docs/plans/012_downloads.md).
        SectionLabel("Downloads")
        val status by app.downloads.status.collectAsState()
        Hint("Saved on ${app.downloads.location()} · ${bytes(status.usedBytes)} used. At the limit, downloads stop; nothing is deleted to make room.")
        Chips(listOf(2, 5, 10, 20, 50, 0), prefs.downloadLimitGb, { if (it == 0) "No limit" else "$it GB" }) {
            prefs.chooseDownloadLimit(it)
            app.downloads.poke()
        }
        Row(
            Modifier
                .fillMaxWidth()
                .clickable {
                    prefs.chooseWifiOnly(!prefs.wifiOnly)
                    app.downloads.poke()
                }.padding(horizontal = 20.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Download on Wi-Fi only", Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge)
            Switch(prefs.wifiOnly, {
                prefs.chooseWifiOnly(it)
                app.downloads.poke()
            })
        }

        SectionLabel("Account")
        Hint("${prefs.userName} on ${prefs.server}" + if (prefs.serverVersion.isEmpty()) "" else " · server ${prefs.serverVersion}")
        OutlinedButton({ app.signOut() }, Modifier.padding(horizontal = 20.dp, vertical = 8.dp)) {
            Icon(Icons.Logout, null, Modifier.size(18.dp))
            Spacer(Modifier.width(8.dp))
            Text("Sign out")
        }
        val error by app.sync.error.collectAsState()
        error?.let { Hint("Last sync failed: $it") }

        Text(
            "Dhun ${BuildConfig.VERSION_NAME} · GPL-3.0",
            Modifier.padding(20.dp),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun Hint(text: String) =
    Text(
        text,
        Modifier.padding(horizontal = 20.dp, vertical = 4.dp),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )

@Composable
fun <T> Chips(
    options: List<T>,
    selected: T,
    label: (T) -> String,
    onPick: (T) -> Unit,
) {
    val c = MaterialTheme.colorScheme
    FlowRow(
        Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        for (o in options) {
            val on = o == selected
            Row(
                Modifier
                    .height(36.dp)
                    .background(if (on) c.primaryContainer else c.surface, RoundedCornerShape(18.dp))
                    .border(BorderStroke(1.dp, if (on) c.primary else c.outline), RoundedCornerShape(18.dp))
                    .clickable { onPick(o) }
                    .padding(horizontal = 14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (on) {
                    Icon(Icons.Check, null, Modifier.size(16.dp), tint = c.onPrimaryContainer)
                    Spacer(Modifier.width(6.dp))
                }
                Text(label(o), style = MaterialTheme.typography.bodyMedium, color = if (on) c.onPrimaryContainer else c.onSurface)
            }
        }
    }
}

@Composable
private fun PaletteSwatch(
    p: Palette,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val c = MaterialTheme.colorScheme
    val accent = if (c.background.luminance() < 0.5f) p.dark else p.light
    Column(Modifier.width(88.dp).clickable(onClick = onClick), horizontalAlignment = Alignment.CenterHorizontally) {
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
            placeholder = { Text("gargantua:8585") },
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
 * What people type is "gargantua" or "192.168.1.101:8585": add the scheme
 * and the default port (docs/plans/003_deployment.md) when they are missing.
 */
fun serverUrl(input: String): String {
    var s = input.trim().trimEnd('/')
    if (!s.contains("://")) s = "http://$s"
    val host = s.substringAfter("://").substringBefore('/')
    if (!host.contains(':') && s.startsWith("http://")) s = s.replaceFirst(host, "$host:8585")
    return s
}
