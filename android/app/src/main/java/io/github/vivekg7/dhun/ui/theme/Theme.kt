package io.github.vivekg7.dhun.ui.theme

import android.app.Activity
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import androidx.core.view.WindowCompat
import kotlinx.coroutines.delay
import java.time.LocalTime

/**
 * The eight accents (docs/plans/011_android_app.md). A palette only sets the
 * accent; the neutrals are shared, so adding one is a single line.
 */
enum class Palette(
    val label: String,
    val light: Color,
    val dark: Color,
) {
    DullOrange("Dull Orange", Color(0xFFC27B45), Color(0xFFD9925C)),
    Sage("Sage", Color(0xFF7E9C7A), Color(0xFF98B594)),
    SlateBlue("Slate Blue", Color(0xFF6A7FA8), Color(0xFF8C9FC6)),
    Teal("Teal", Color(0xFF4E9A96), Color(0xFF6FB6B1)),
    DustyRose("Dusty Rose", Color(0xFFB9707F), Color(0xFFD08D9B)),
    Mauve("Mauve", Color(0xFF9479A8), Color(0xFFB097C2)),
    Olive("Olive", Color(0xFF8F9152), Color(0xFFADAF6F)),
    Sand("Sand", Color(0xFFB59E73), Color(0xFFCBB68C)),
}

enum class ThemeMode(
    val label: String,
) {
    System("Follow system"),
    Light("Light"),
    Dark("Dark"),
    Black("Black"),
    DayNight("Day/Night by time"),
}

/** Day/Night by time: light from 07:00 to 19:00. */
private fun isDaytime() = LocalTime.now().hour in 7 until 19

private val Ink = Color(0xFF1D1B19)

fun lightScheme(p: Palette): ColorScheme =
    lightColorScheme(
        primary = p.light,
        onPrimary = Color.White,
        primaryContainer = lerp(p.light, Color.White, 0.8f),
        onPrimaryContainer = lerp(p.light, Ink, 0.6f),
        // Accent-coloured text (section labels): darker than the accent, so it reads on white.
        secondary = lerp(p.light, Ink, 0.25f),
        onSecondary = Color.White,
        // Every other role follows the accent too, so no Material default (purple) shows through.
        secondaryContainer = lerp(p.light, Color.White, 0.8f),
        onSecondaryContainer = lerp(p.light, Ink, 0.6f),
        tertiary = lerp(p.light, Ink, 0.25f),
        tertiaryContainer = lerp(p.light, Color.White, 0.8f),
        onTertiaryContainer = lerp(p.light, Ink, 0.6f),
        surfaceTint = p.light,
        inversePrimary = p.dark,
        background = Color.White,
        onBackground = Ink,
        surface = Color.White,
        onSurface = Ink,
        onSurfaceVariant = Color(0xFF6F6862),
        surfaceContainerLowest = Color.White,
        surfaceContainerLow = Color(0xFFFAF7F4),
        surfaceContainer = Color(0xFFF5F0EC),
        surfaceContainerHigh = Color(0xFFEFE9E4),
        surfaceContainerHighest = Color(0xFFE8E1DB),
        surfaceVariant = Color(0xFFEFE9E4),
        outline = Color(0xFFDDD3CB),
        outlineVariant = Color(0xFFEEE8E3),
    )

fun darkScheme(
    p: Palette,
    black: Boolean,
): ColorScheme {
    val bg = if (black) Color.Black else Color(0xFF141211)
    return darkColorScheme(
        primary = p.dark,
        onPrimary = Color(0xFF1A1410),
        primaryContainer = lerp(p.dark, bg, 0.72f),
        onPrimaryContainer = lerp(p.dark, Color.White, 0.35f),
        secondary = p.dark,
        onSecondary = Color(0xFF1A1410),
        secondaryContainer = lerp(p.dark, bg, 0.72f),
        onSecondaryContainer = lerp(p.dark, Color.White, 0.35f),
        tertiary = p.dark,
        tertiaryContainer = lerp(p.dark, bg, 0.72f),
        onTertiaryContainer = lerp(p.dark, Color.White, 0.35f),
        surfaceTint = p.dark,
        inversePrimary = p.light,
        background = bg,
        onBackground = Color(0xFFEDE7E2),
        surface = bg,
        onSurface = Color(0xFFEDE7E2),
        onSurfaceVariant = Color(0xFFA39A92),
        surfaceContainerLowest = bg,
        surfaceContainerLow = if (black) Color(0xFF0A0A0A) else Color(0xFF181614),
        surfaceContainer = if (black) Color(0xFF111111) else Color(0xFF1C1917),
        surfaceContainerHigh = if (black) Color(0xFF181818) else Color(0xFF211D1A),
        surfaceContainerHighest = if (black) Color(0xFF222222) else Color(0xFF2A2522),
        surfaceVariant = if (black) Color(0xFF181818) else Color(0xFF211D1A),
        outline = if (black) Color(0xFF2E2E2E) else Color(0xFF3A3430),
        outlineVariant = if (black) Color(0xFF1E1E1E) else Color(0xFF2A2522),
    )
}

private val type =
    Typography().run {
        copy(
            headlineSmall = headlineSmall.copy(fontWeight = FontWeight.Bold, fontSize = 22.sp, letterSpacing = (-0.01).em),
            titleLarge = titleLarge.copy(fontWeight = FontWeight.Bold, fontSize = 24.sp, letterSpacing = (-0.01).em),
            titleMedium = titleMedium.copy(fontWeight = FontWeight.Medium, fontSize = 16.sp),
            bodyMedium = bodyMedium.copy(fontSize = 14.sp),
            labelSmall = TextStyle(fontWeight = FontWeight.Medium, fontSize = 12.sp, letterSpacing = 0.1.em),
        )
    }

@Composable
fun DhunTheme(
    mode: ThemeMode,
    palette: Palette,
    content: @Composable () -> Unit,
) {
    // Re-read the clock every minute, so Day/Night switches on its own.
    var minute by remember { mutableIntStateOf(0) }
    if (mode == ThemeMode.DayNight) {
        LaunchedEffect(Unit) {
            while (true) {
                delay(60_000)
                minute++
            }
        }
    }
    val systemDark = isSystemInDarkTheme()
    val dark =
        when (mode) {
            ThemeMode.System -> systemDark
            ThemeMode.Light -> false
            ThemeMode.Dark, ThemeMode.Black -> true
            ThemeMode.DayNight -> minute >= 0 && !isDaytime()
        }
    val scheme = if (dark) darkScheme(palette, black = mode == ThemeMode.Black) else lightScheme(palette)
    val view = LocalView.current
    if (!view.isInEditMode) {
        SideEffect {
            val window = (view.context as Activity).window
            WindowCompat.getInsetsController(window, view).apply {
                isAppearanceLightStatusBars = !dark
                isAppearanceLightNavigationBars = !dark
            }
        }
    }
    MaterialTheme(colorScheme = scheme, typography = type, content = content)
}
