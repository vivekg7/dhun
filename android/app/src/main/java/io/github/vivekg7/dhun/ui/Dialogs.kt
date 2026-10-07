package io.github.vivekg7.dhun.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
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

/** A small heading over a group of rows. */
@Composable
fun DialogLabel(text: String) =
    Text(
        text,
        Modifier.padding(top = 12.dp, bottom = 4.dp),
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.primary,
    )

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

/** "Other…": a number the presets don't have, in [range], with its [unit] beside it. */
@Composable
fun NumberDialog(
    title: String,
    unit: String,
    initial: Int,
    range: IntRange,
    onDismiss: () -> Unit,
    onSet: (Int) -> Unit,
) {
    var text by remember { mutableStateOf("$initial") }
    val n = text.toIntOrNull()?.takeIf { it in range }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    text,
                    { text = it.filter(Char::isDigit).take(4) },
                    Modifier.width(112.dp).focusRequester(focus),
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Done),
                )
                Spacer(Modifier.width(12.dp))
                Text(unit, style = MaterialTheme.typography.bodyLarge)
            }
        },
        confirmButton = { TextButton({ n?.let(onSet) }, enabled = n != null) { Text("Set") } },
        dismissButton = { TextButton(onDismiss) { Text("Cancel") } },
    )
}
