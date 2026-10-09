package io.github.vivekg7.dhun.ui

import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.data.Covers
import io.github.vivekg7.dhun.data.Song

@Composable
fun Art(
    song: Song?,
    size: Dp,
    shape: Shape,
    modifier: Modifier = Modifier,
) {
    val px = with(LocalDensity.current) { size.roundToPx() }
    var image by remember(song?.id) { mutableStateOf(song?.let { Covers.cached(it, px) }) }
    if (song != null && song.hasArt) {
        LaunchedEffect(song.id, song.art, px) {
            // A larger view shows the thumbnail at once, then the cover when it comes; offline, the thumbnail stays.
            // The same bitmap again is no change, so the cover does not fade into itself.
            if (px > Covers.THUMB && image == null) Covers.thumb(song)?.let { image = it }
            Covers.load(song, px)?.let { if (it !== image) image = it }
        }
    }
    Box(
        modifier.size(size).clip(shape).background(MaterialTheme.colorScheme.surfaceContainerHigh),
        contentAlignment = Alignment.Center,
    ) {
        // The cover fades in over the note, rather than replacing it from one frame to the next.
        Crossfade(image, animationSpec = tween(Motion.SHORT), label = "art") { img ->
            if (img != null) {
                Image(remember(img) { img.asImageBitmap() }, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
            } else {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Icon(Icons.Note, null, Modifier.size(size * 0.4f), tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f))
                }
            }
        }
    }
}

val SmallArt = 44.dp
