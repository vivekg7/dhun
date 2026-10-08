package io.github.vivekg7.dhun.data

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.LruCache
import io.github.vivekg7.dhun.App
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import okhttp3.Call
import okhttp3.Callback
import okhttp3.Request
import okhttp3.Response
import java.io.File
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import kotlin.coroutines.resume

/**
 * Cover art (docs/plans/019_networking_and_caching.md), kept as files named
 * by the server's art key: songs showing one cover share it, so an album's
 * cover is fetched and stored once, and a new cover comes with a new key, so
 * a kept one never needs asking about again and shows offline. Each is
 * fetched when first shown; the [KEEP] used most recently stay, and those of
 * downloaded songs always. Decoded bitmaps stay in memory while they fit.
 * The server already resizes, so no image library (docs/plans/011_android_app.md).
 */
object Covers {
    /** The owner's figure: enough for everything played in a long while, without fetching the whole library. */
    private const val KEEP = 500

    /** The server's sizes (`/art/{id}?size=`). */
    private val SIZES = listOf(128, 256, 512, 1024)

    private val app get() = App.app
    private val dir by lazy { File(app.filesDir, "covers").apply { mkdirs() } }

    private val memory =
        object : LruCache<String, Bitmap>(32 shl 20) {
            override fun sizeOf(
                key: String,
                value: Bitmap,
            ) = value.byteCount
        }

    /** Fetches under way, shared by every list row showing that cover. */
    private val fetches = HashMap<String, Fetch>()

    private class Fetch(
        val job: Deferred<File?>,
    ) {
        var waiters = 0
    }

    /** Keys whose file was marked used in this run: once is enough to order them. */
    private val touched = ConcurrentHashMap.newKeySet<String>()
    private var written = 0

    /** A server without art keys (before 0.1.4): one cover per song. */
    private fun key(song: Song) = song.art.ifEmpty { "s${song.id}" }

    /** [song]'s cover, at least [px] across where the server has it that large; null if it has none or it cannot be had now. */
    suspend fun load(
        song: Song,
        px: Int,
    ): Bitmap? {
        if (!song.hasArt) return null
        val size = SIZES.firstOrNull { it >= px } ?: SIZES.last()
        val key = key(song)
        val mem = "$key/$size"
        memory.get(mem)?.let { return it }
        return withContext(Dispatchers.IO) {
            // Offline, a smaller copy beats none.
            val file = kept(key, size) ?: fetch(song.id, key, size) ?: kept(key, 0)
            file?.let { decode(it, px) }?.also { memory.put(mem, it) }
        }
    }

    /** The kept file for [key] of at least [size] px. One per key: a larger fetch replaces it. */
    private fun kept(
        key: String,
        size: Int,
    ): File? {
        val file = SIZES.filter { it >= size }.map { File(dir, "$key.$it") }.firstOrNull { it.exists() } ?: return null
        if (touched.add(key)) file.setLastModified(System.currentTimeMillis())
        return file
    }

    /**
     * One request per cover, however many rows want it. It is cancelled
     * when the last of them goes, so covers scrolled past stop loading and
     * the ones on screen come first.
     */
    private suspend fun fetch(
        song: Long,
        key: String,
        size: Int,
    ): File? {
        val name = "$key.$size"
        val f =
            synchronized(fetches) {
                fetches.getOrPut(name) { Fetch(app.scope.async(Dispatchers.IO) { download(song, key, size) }) }.also { it.waiters++ }
            }
        try {
            return f.job.await()
        } finally {
            synchronized(fetches) {
                if (--f.waiters == 0) {
                    fetches.remove(name)
                    f.job.cancel()
                }
            }
        }
    }

    private suspend fun download(
        song: Long,
        key: String,
        size: Int,
    ): File? {
        val bytes =
            app.api.http
                .newCall(Request.Builder().url(app.api.artUrl(song, size)).build())
                .bytes() ?: return null
        val file = File(dir, "$key.$size")
        val tmp = File(dir, "$key.$size.tmp")
        tmp.writeBytes(bytes)
        if (!tmp.renameTo(file)) return null.also { tmp.delete() }
        touched.add(key)
        for (s in SIZES) if (s < size) File(dir, "$key.$s").delete()
        if (written++ % 25 == 0) evict()
        return file
    }

    /** Drops the covers used longest ago beyond [KEEP], never a downloaded song's. */
    private suspend fun evict() {
        val files = dir.listFiles()?.filter { !it.name.endsWith(".tmp") } ?: return
        if (files.size <= KEEP) return
        val byId = app.catalog.value.byId
        val downloaded =
            app.db
                .dao()
                .downloads()
                .first()
                .mapNotNull { byId[it.song]?.let(::key) }
                .toSet()
        files
            .filter { it.name.substringBeforeLast('.') !in downloaded }
            .sortedBy { it.lastModified() }
            .take(files.size - KEEP)
            .forEach { it.delete() }
    }

    /** Decodes at the smallest power-of-two step still [px] across: a list row needs a quarter of a grid tile. */
    private fun decode(
        file: File,
        px: Int,
    ): Bitmap? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.path, bounds)
        var sample = 1
        while (bounds.outWidth / (sample * 2) >= px && bounds.outHeight / (sample * 2) >= px) sample *= 2
        return BitmapFactory.decodeFile(file.path, BitmapFactory.Options().apply { inSampleSize = sample })
    }
}

/** The body, or null on a failed answer or no network; cancelling the coroutine cancels the request. */
private suspend fun Call.bytes(): ByteArray? =
    suspendCancellableCoroutine { cont ->
        cont.invokeOnCancellation { cancel() }
        enqueue(
            object : Callback {
                override fun onFailure(
                    call: Call,
                    e: IOException,
                ) = cont.resume(null)

                override fun onResponse(
                    call: Call,
                    response: Response,
                ) {
                    val body = runCatching { response.use { if (it.isSuccessful) it.body.bytes() else null } }.getOrNull()
                    cont.resume(body)
                }
            },
        )
    }
