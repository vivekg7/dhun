package io.github.vivekg7.dhun.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.data.Catalog
import io.github.vivekg7.dhun.data.Group
import io.github.vivekg7.dhun.data.SearchFilters
import io.github.vivekg7.dhun.data.SearchFilters.Sort
import io.github.vivekg7.dhun.data.SearchIndex
import io.github.vivekg7.dhun.data.SearchQuery
import java.time.Year

/**
 * The filters under the Search tab's box (docs/plans/041_search_filters_and_popularity.md):
 * one chip each, naming what it is set to, opening a dialog to set it.
 * Always in view, so what is applied is never hidden.
 */
@Composable
fun SearchFilterChips(
    filters: SearchFilters,
    catalog: Catalog,
    onChange: (SearchFilters) -> Unit,
) {
    var open by remember { mutableStateOf("") }
    LazyRow(
        Modifier.fillMaxWidth().padding(bottom = 4.dp),
        contentPadding = PaddingValues(horizontal = 16.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        item { Chip(filters.years?.let(SearchFilters::yearsLabel), "Year") { open = "year" } }
        item { Chip(SearchFilters.names(filters.artists), "Artist") { open = "artist" } }
        item { Chip(SearchFilters.names(filters.genres), "Genre") { open = "genre" } }
        item { FilterChip(filters.popular, { onChange(filters.copy(popular = !filters.popular)) }, { Text("Popular") }) }
        item { Chip(filters.sort.label.takeIf { filters.sort != Sort.Best }, "Sort") { open = "sort" } }
    }
    val close = { open = "" }
    when (open) {
        "year" -> {
            YearDialog(filters.years, close) { onChange(filters.copy(years = it)) }
        }

        // The indexes are built when first opened, not with the tab.
        "artist" -> {
            PickDialog("Artists", catalog.artistIndex, filters.artists, close) { onChange(filters.copy(artists = it)) }
        }

        "genre" -> {
            PickDialog("Genres", catalog.genreIndex, filters.genres, close) { onChange(filters.copy(genres = it)) }
        }

        "sort" -> {
            AlertDialog(
                close,
                confirmButton = {},
                dismissButton = { TextButton(close) { Text("Cancel") } },
                title = { Text("Sort songs") },
                text = {
                    Column {
                        for (s in Sort.entries) {
                            DialogRow(s.label, s == filters.sort) {
                                close()
                                onChange(filters.copy(sort = s))
                            }
                        }
                    }
                },
            )
        }
    }
}

/** A chip that shows its [value] once set, and its [name] until then. */
@Composable
private fun Chip(
    value: String?,
    name: String,
    onClick: () -> Unit,
) = FilterChip(
    value != null,
    onClick,
    { Text(value ?: name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
    trailingIcon = { Icon(Icons.ExpandMore, null, Modifier.size(18.dp)) },
)

/** Decades a tap away, or any range typed in. */
@Composable
private fun YearDialog(
    years: IntRange?,
    onDismiss: () -> Unit,
    onPick: (IntRange?) -> Unit,
) {
    val now = Year.now().value
    val decades = (now / 10 * 10 downTo 1950 step 10).map { it..it + 9 }
    var from by remember { mutableStateOf(years?.first?.toString().orEmpty()) }
    var to by remember { mutableStateOf(years?.last?.toString().orEmpty()) }

    fun pick(r: IntRange?) {
        onDismiss()
        onPick(r)
    }
    AlertDialog(
        onDismiss,
        confirmButton = {
            TextButton({
                val a = from.toIntOrNull()
                val b = to.toIntOrNull()
                pick(if (a == null && b == null) null else minOf(a ?: 1, b ?: now)..maxOf(a ?: 1, b ?: now))
            }) { Text("Apply") }
        },
        dismissButton = { TextButton({ pick(null) }) { Text("Any year") } },
        title = { Text("Year") },
        text = {
            Column {
                LazyColumn(Modifier.heightIn(max = 280.dp)) {
                    items(decades) { d -> DialogRow(SearchFilters.yearsLabel(d), d == years) { pick(d) } }
                    item { DialogRow("Before 1950", years == 1..1949) { pick(1..1949) } }
                }
                Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    val number = KeyboardOptions(keyboardType = KeyboardType.Number)
                    OutlinedTextField(
                        from,
                        { from = it.filter(Char::isDigit).take(4) },
                        Modifier.weight(1f),
                        label = { Text("From") },
                        singleLine = true,
                        keyboardOptions = number,
                    )
                    Spacer(Modifier.width(12.dp))
                    OutlinedTextField(
                        to,
                        { to = it.filter(Char::isDigit).take(4) },
                        Modifier.weight(1f),
                        label = { Text("To") },
                        singleLine = true,
                        keyboardOptions = number,
                    )
                }
            }
        },
    )
}

/**
 * Several artists or genres, picked from the library's list and searched
 * as the tab searches them. The ones picked stay at the top.
 */
@Composable
private fun PickDialog(
    title: String,
    index: SearchIndex<Group>,
    picked: Set<String>,
    onDismiss: () -> Unit,
    onPick: (Set<String>) -> Unit,
) {
    var chosen by remember { mutableStateOf(picked) }
    var text by remember { mutableStateOf("") }
    val shown =
        remember(text) {
            val all = index.filter(SearchQuery(text)).map { it.value }
            val first = picked.mapNotNull { p -> index.items.firstOrNull { it.name.equals(p, ignoreCase = true) } }
            // Most songs first: the names people look for are the big ones.
            (first + all.filterNot { g -> picked.any { it.equals(g.name, ignoreCase = true) } }.sortedByDescending { it.songs.size })
        }
    AlertDialog(
        onDismiss,
        confirmButton = {
            TextButton({
                onDismiss()
                onPick(chosen)
            }) { Text("Apply") }
        },
        dismissButton = {
            TextButton({
                onDismiss()
                onPick(emptySet())
            }) { Text("Clear") }
        },
        title = { Text(title) },
        text = {
            Column {
                OutlinedTextField(text, { text = it }, Modifier.fillMaxWidth(), placeholder = { Text("Search") }, singleLine = true)
                LazyColumn(Modifier.heightIn(max = 360.dp).padding(top = 8.dp)) {
                    items(shown, key = { it.name.lowercase() }) { g ->
                        val on = chosen.any { it.equals(g.name, ignoreCase = true) }
                        Row(
                            Modifier
                                .fillMaxWidth()
                                .heightIn(min = 48.dp)
                                .clickable { chosen = if (on) chosen.filterNot { it.equals(g.name, ignoreCase = true) }.toSet() else chosen + g.name },
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Checkbox(on, null)
                            Spacer(Modifier.width(12.dp))
                            Text(g.name, Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text("${g.songs.size}", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
            }
        },
    )
}
