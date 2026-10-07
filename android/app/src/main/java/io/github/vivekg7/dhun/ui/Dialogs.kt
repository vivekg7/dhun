package io.github.vivekg7.dhun.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

/*
 * The one pattern for the app's dialogs (docs/plans/018_settings_layout.md):
 * a title, perhaps a line saying what it does, then one-line rows, and text
 * buttons below. The dialog gives the margin, so nothing in it adds its own
 * inset: rows sit flush with the title.
 */

/** A line under the title: what the dialog does, or what is set now. */
@Composable
fun DialogNote(
    text: String,
    color: Color = MaterialTheme.colorScheme.onSurfaceVariant,
) = Text(text, Modifier.padding(bottom = 8.dp), style = MaterialTheme.typography.bodyMedium, color = color)

/**
 * One row: a choice when [selected] is given (with its radio button), else
 * an action. [icon] and [detail] are for rows that name things, like playlists.
 */
@Composable
fun DialogRow(
    text: String,
    selected: Boolean? = null,
    icon: ImageVector? = null,
    detail: String? = null,
    onClick: () -> Unit,
) {
    val c = MaterialTheme.colorScheme
    Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).clickable(onClick = onClick), verticalAlignment = Alignment.CenterVertically) {
        if (selected != null) {
            RadioButton(selected, null)
            Spacer(Modifier.width(16.dp))
        }
        if (icon != null) {
            Icon(icon, null, Modifier.size(22.dp), tint = c.onSurfaceVariant)
            Spacer(Modifier.width(16.dp))
        }
        Text(text, Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
        detail?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = c.onSurfaceVariant) }
    }
}
