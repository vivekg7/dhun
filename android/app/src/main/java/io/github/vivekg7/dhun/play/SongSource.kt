package io.github.vivekg7.dhun.play

import android.net.Uri
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.TransferListener
import io.github.vivekg7.dhun.App
import io.github.vivekg7.dhun.data.SongCache
import java.io.RandomAccessFile

/**
 * Opens a song where it is on the phone (docs/plans/019_networking_and_caching.md):
 * a download, then a whole file in the song cache, then one the cache is
 * still fetching, read as it arrives; the stream only for a seek far past
 * what has arrived, or with the cache off. Resolving at open time means a
 * download or a fetch that finishes while the queue is loaded is used too.
 */
@OptIn(UnstableApi::class)
class SongSource(
    private val app: App,
    /** Files and the stream. */
    private val other: DataSource,
) : DataSource {
    private var growing: SongCache.Entry? = null
    private var file: RandomAccessFile? = null
    private var position = 0L
    private var remaining = 0L
    private var uri: Uri? = null

    override fun addTransferListener(transferListener: TransferListener) = other.addTransferListener(transferListener)

    override fun open(dataSpec: DataSpec): Long {
        uri = dataSpec.uri
        val id =
            dataSpec.uri.lastPathSegment
                ?.toLongOrNull()
                ?.takeIf { dataSpec.uri.toString() == app.api.streamUrl(it) }
        val local = id?.let { app.downloads.file(it) ?: app.cache.complete(it) }
        val opened = id?.takeIf { local == null }?.let { app.cache.open(it, dataSpec.position) }
        if (opened == null) return other.open(if (local != null) dataSpec.withUri(Uri.fromFile(local)) else dataSpec)
        val (entry, raf) = opened
        growing = entry
        file = raf
        position = dataSpec.position
        remaining =
            when {
                dataSpec.length != C.LENGTH_UNSET.toLong() -> dataSpec.length
                entry.total > 0 -> entry.total - position
                else -> C.LENGTH_UNSET.toLong()
            }
        return remaining
    }

    override fun read(
        buffer: ByteArray,
        offset: Int,
        length: Int,
    ): Int {
        val e = growing ?: return other.read(buffer, offset, length)
        if (length == 0) return 0
        if (remaining == 0L) return C.RESULT_END_OF_INPUT
        val arrived = e.awaitPast(position)
        if (arrived <= position) return C.RESULT_END_OF_INPUT
        var n = minOf(length.toLong(), arrived - position)
        if (remaining != C.LENGTH_UNSET.toLong()) n = minOf(n, remaining)
        val f = file!!
        f.seek(position)
        val got = f.read(buffer, offset, n.toInt())
        if (got < 0) return C.RESULT_END_OF_INPUT
        position += got
        if (remaining != C.LENGTH_UNSET.toLong()) remaining -= got
        return got
    }

    override fun getUri(): Uri? = if (growing != null) uri else other.uri

    override fun getResponseHeaders(): Map<String, List<String>> = if (growing != null) emptyMap() else other.responseHeaders

    override fun close() {
        if (growing == null) return other.close()
        growing = null
        file?.close()
        file = null
    }
}
