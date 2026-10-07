package io.github.vivekg7.dhun.ui

import android.graphics.BitmapFactory
import android.util.LruCache
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
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.Song
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.Request

/**
 * Cover art: the server already resizes it (/art/{id}?size=), OkHttp keeps it
 * on disk, and decoded bitmaps stay in memory while they fit. That is all a
 * list of covers needs, so no image library (docs/plans/011_android_app.md).
 */
private object ArtCache {
    val memory =
        object : LruCache<String, ImageBitmap>(32 shl 20) {
            override fun sizeOf(
                key: String,
                value: ImageBitmap,
            ) = value.width * value.height * 4
        }

    suspend fun load(
        song: Long,
        px: Int,
    ): ImageBitmap? {
        // The server's sizes; asking for one of them lets the disk cache hit.
        val size = listOf(128, 256, 512, 1024).firstOrNull { it >= px } ?: 1024
        val key = "$song/$size"
        memory.get(key)?.let { return it }
        return withContext(Dispatchers.IO) {
            runCatching {
                val app = App.app
                app.api.http.newCall(Request.Builder().url(app.api.artUrl(song, size)).build()).execute().use { resp ->
                    if (!resp.isSuccessful) return@use null
                    BitmapFactory.decodeStream(resp.body.byteStream())?.asImageBitmap()
                }
            }.getOrNull()?.also { memory.put(key, it) }
        }
    }
}

@Composable
fun Art(
    song: Song?,
    size: Dp,
    shape: Shape,
    modifier: Modifier = Modifier,
) {
    val px = with(LocalDensity.current) { size.roundToPx() }
    var image by remember(song?.id) { mutableStateOf<ImageBitmap?>(null) }
    if (song != null && song.hasArt) LaunchedEffect(song.id, px) { image = ArtCache.load(song.id, px) }
    Box(
        modifier.size(size).clip(shape).background(MaterialTheme.colorScheme.surfaceContainerHigh),
        contentAlignment = Alignment.Center,
    ) {
        val img = image
        if (img != null) {
            Image(img, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        } else {
            Icon(Icons.Note, null, Modifier.size(size * 0.4f), tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f))
        }
    }
}

val SmallArt = 44.dp
