package io.github.vivekg7.dhun.data

import java.text.Normalizer

/**
 * The library as the tabs show it, derived from the song list in memory.
 * About 7,000 songs group in a few milliseconds, which is simpler and faster
 * than keeping albums, artists and folders as tables.
 */
class Catalog(
    all: List<Song>,
) {
    val songs: List<Song> = all.sortedWith(compareBy(TITLE) { it.title })
    val byId: Map<Long, Song> = songs.associateBy { it.id }

    /**
     * Two songs are on one album when they share the album name and either
     * the album artist or, without one, the folder. A bare album name would
     * merge every "Greatest Hits" in the collection. A phone copy of a NAS
     * album is an album of its own, not each track twice in one
     * (docs/plans/025_phone_local_songs.md).
     */
    val albums: List<Album> by lazy {
        songs
            .filter { it.album.isNotEmpty() }
            .groupBy { (if (it.onPhone) "phone\u0000" else "") + it.album.lowercase() + '\u0000' + it.albumArtist.lowercase().ifEmpty { it.folder } }
            .map { (key, list) ->
                val sorted = list.sortedWith(compareBy<Song>({ it.disc }, { it.track }).thenBy(TITLE) { it.title })
                val first = sorted.first()
                Album(key, first.album, first.albumArtist.ifEmpty { mostCommon(sorted.map { it.displayArtist }) }, sorted.maxOf { it.year }, sorted)
            }.sortedWith(compareBy(TITLE) { it.name })
    }

    val artists: List<Group> by lazy { group { it.artistList.ifEmpty { listOf(it.displayArtist) } } }
    val genres: List<Group> by lazy { group { it.genreList.ifEmpty { listOf("Unknown genre") } } }

    /** Every folder, keyed by its path relative to the Music folder ("" is the top). */
    val folders: Map<String, Folder> by lazy {
        val map = HashMap<String, Folder>()

        fun folder(path: String): Folder =
            map.getOrPut(path) {
                Folder(path).also { if (path.isNotEmpty()) folder(path.substringBeforeLast('/', "")).children += it }
            }
        for (s in songs) folder(s.folder).songs += s
        for (f in map.values) {
            // The phone's songs first, before the NAS folders.
            f.children.sortWith(compareBy<Folder> { it.path != LocalSongs.PHONE_ROOT }.thenBy(TITLE) { it.name })
            f.songs.sortWith(compareBy<Song>({ it.disc }, { it.track }).thenBy(TITLE) { it.path })
        }
        folder("")
        map
    }

    fun songsOf(ids: List<Long>) = ids.mapNotNull { byId[it] }

    /** False until the NAS songs are read: phone songs alone are not the library loaded. */
    val hasNas: Boolean by lazy { songs.any { !it.onPhone } }

    /** Title, album and artist, ignoring case and accents; titles that start with the query first. */
    fun search(query: String): List<Song> {
        val q = fold(query.trim())
        if (q.isEmpty()) return emptyList()
        val hits = songs.filter { s -> fold(s.title).contains(q) || fold(s.album).contains(q) || fold(s.artist).contains(q) }
        return hits.sortedBy {
            if (fold(it.title).startsWith(q)) {
                0
            } else if (fold(it.title).contains(q)) {
                1
            } else {
                2
            }
        }
    }

    private fun group(keys: (Song) -> List<String>): List<Group> {
        val map = HashMap<String, MutableList<Song>>()
        val names = HashMap<String, String>()
        for (s in songs) {
            for (k in keys(s)) {
                val lower = k.lowercase()
                names.putIfAbsent(lower, k)
                map.getOrPut(lower) { mutableListOf() } += s
            }
        }
        return map.map { (k, list) -> Group(names.getValue(k), list) }.sortedWith(compareBy(TITLE) { it.name })
    }

    companion object {
        /** Natural, case-blind order: "Track 2" before "Track 10". */
        val TITLE: Comparator<String> = Comparator { a, b -> natural(a, b) }

        private fun natural(
            a: String,
            b: String,
        ): Int {
            var i = 0
            var j = 0
            while (i < a.length && j < b.length) {
                val ca = a[i]
                val cb = b[j]
                if (ca.isDigit() && cb.isDigit()) {
                    val si = i
                    val sj = j
                    while (i < a.length && a[i].isDigit()) i++
                    while (j < b.length && b[j].isDigit()) j++
                    val na = a.substring(si, i).trimStart('0')
                    val nb = b.substring(sj, j).trimStart('0')
                    if (na.length != nb.length) return na.length - nb.length
                    val c = na.compareTo(nb)
                    if (c != 0) return c
                } else {
                    val c = ca.lowercaseChar().compareTo(cb.lowercaseChar())
                    if (c != 0) return c
                    i++
                    j++
                }
            }
            return (a.length - i) - (b.length - j)
        }

        private val MARKS = Regex("\\p{Mn}+")

        fun fold(s: String) = MARKS.replace(Normalizer.normalize(s, Normalizer.Form.NFD), "").lowercase()

        private fun mostCommon(names: List<String>) =
            names
                .groupingBy { it }
                .eachCount()
                .maxBy { it.value }
                .key
    }
}

data class Album(
    val key: String,
    val name: String,
    val artist: String,
    val year: Int,
    val songs: List<Song>,
)

data class Group(
    val name: String,
    val songs: List<Song>,
)

class Folder(
    val path: String,
) {
    val name get() = path.substringAfterLast('/').ifEmpty { "Music" }
    val children = mutableListOf<Folder>()
    val songs = mutableListOf<Song>()

    /** This folder's songs and every subfolder's, in folder order: what "play folder" plays. */
    fun allSongs(): List<Song> = songs + children.flatMap { it.allSongs() }
}
