package io.github.vivekg7.dhun.data

/**
 * The library as the tabs show it, derived from the song list in memory.
 * About 7,000 songs group in a few milliseconds, which is simpler and faster
 * than keeping albums, artists and folders as tables.
 */
class Catalog(
    all: List<Song>,
) {
    /**
     * The music: what every song list, view and search of songs shows.
     * Podcasts and audiobooks have their own pages ([books], [shows]), so
     * an episode never turns up in an artist or a shuffle
     * (docs/plans/031_podcasts_and_audiobooks.md).
     */
    val songs: List<Song> = all.filter { !it.spoken }.sortedWith(compareBy(TITLE) { it.title })

    /** Every song, episodes and books' files too: what queues, Favorites and Listen Later name. */
    val byId: Map<Long, Song> = all.associateBy { it.id }

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
    val hasNas: Boolean by lazy { byId.values.any { !it.onPhone } }

    /** Audiobooks and podcast shows as the server grouped them, each one's files in its play order. */
    val books: List<Shelf> by lazy { shelves(all, Song.AUDIOBOOK) }
    val shows: List<Shelf> by lazy { shelves(all, Song.PODCAST) }
    val shelves: Map<String, Shelf> by lazy { (books + shows).associateBy { it.key } }

    /** The shelf [s] is on, or null for music. */
    fun shelfOf(s: Song) = if (s.spoken) shelves[shelfKey(s)] else null

    // Built on first use, with the catalog, so a keystroke folds nothing (docs/plans/029_search.md).
    val songIndex by lazy { SearchIndex.songs(songs) }
    val albumIndex by lazy { SearchIndex(albums, listOf(Field<Album>("album", 0) { it.name }, Field("artist", 1) { it.artist })) }
    val artistIndex by lazy { SearchIndex.names(artists, "artist") { it.name } }
    val genreIndex by lazy { SearchIndex.names(genres, "genre") { it.name } }
    val folderIndex by lazy {
        SearchIndex.names(folders.values.filter { it.path.isNotEmpty() }.sortedWith(compareBy(TITLE) { it.path }), "folder") { it.name }
    }
    val bookIndex by lazy { SearchIndex(books, listOf(Field<Shelf>("book", 0) { it.title }, Field("artist", 1) { it.author })) }
    val showIndex by lazy { SearchIndex.names(shows, "show") { it.title } }
    val episodeIndex by lazy { SearchIndex.songs(shows.flatMap { it.items }) }

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
        /** A file the server did not group (none should be) is a shelf of its folder. */
        private fun shelfKey(s: Song) = s.groupKey.ifEmpty { s.kind + '\u0000' + s.folder }

        private fun shelves(
            all: List<Song>,
            kind: String,
        ): List<Shelf> =
            all
                .filter { it.kind == kind }
                .groupBy(::shelfKey)
                .map { (key, list) ->
                    val items = list.sortedWith(compareBy<Song> { it.groupIndex }.thenBy(TITLE) { it.path })
                    val first = items.first()
                    Shelf(
                        key,
                        kind,
                        first.groupTitle.ifEmpty {
                            first.folder.substringAfterLast('/')
                        },
                        mostCommon(items.map { it.artist.ifEmpty { it.albumArtist } }.filter { it.isNotEmpty() }.ifEmpty { listOf("") }),
                        items,
                    )
                }.sortedWith(compareBy(TITLE) { it.title })

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

/**
 * An audiobook or a podcast show (docs/plans/031_podcasts_and_audiobooks.md):
 * a book's files in reading order, a show's episodes oldest first.
 */
data class Shelf(
    val key: String,
    val kind: String,
    val title: String,
    /** The author, or the host; "" when the tags name none. */
    val author: String,
    val items: List<Song>,
) {
    val isBook get() = kind == Song.AUDIOBOOK

    /** The first file with a cover: the one its files without one show. */
    val cover get() = items.firstOrNull { it.hasArt }
}

class Folder(
    val path: String,
) {
    val name get() = path.substringAfterLast('/').ifEmpty { "Music" }
    val children = mutableListOf<Folder>()
    val songs = mutableListOf<Song>()

    /** This folder's songs and every subfolder's, in folder order: what "play folder" plays. */
    fun allSongs(): List<Song> = songs + children.flatMap { it.allSongs() }
}
