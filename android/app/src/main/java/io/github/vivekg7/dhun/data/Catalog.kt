package io.github.vivekg7.dhun.data

/**
 * The library as the tabs show it, derived from the song list in memory:
 * simpler and faster than keeping albums, artists and folders as tables.
 * About 30,000 songs today, built for 100,000: the sorts compute one key
 * per item and the groupings are built once, off the main thread
 * (docs/plans/036_fast_at_100k.md).
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
    val songs: List<Song> = all.filter { !it.spoken }.sortedNatural { it.title }

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
                val sorted = list.sortedNatural(TRACK) { it.title }
                val first = sorted.first()
                Album(key, first.album, first.albumArtist.ifEmpty { mostCommon(sorted.map { it.displayArtist }) }, sorted.maxOf { it.year }, sorted)
            }.sortedNatural { it.name }
    }

    val artists: List<Group> by lazy { group { it.artistList.ifEmpty { listOf(it.displayArtist) } } }
    val genres: List<Group> by lazy { group { it.genreList.ifEmpty { listOf("Unknown genre") } } }

    /**
     * Every folder, keyed by its path relative to the Music folder ("" is the
     * top). Podcasts and audiobooks are browsed here too, under a root of
     * their own after the music, as their own folders sit beside `Music/` on
     * the NAS; their paths are relative to those folders.
     */
    val folders: Map<String, Folder> by lazy {
        val map = HashMap<String, Folder>()

        fun folder(path: String): Folder =
            map.getOrPut(path) {
                Folder(path).also { if (path.isNotEmpty()) folder(path.substringBeforeLast('/', "")).children += it }
            }
        for (s in songs) folder(s.folder).songs += s
        for (s in all) if (s.spoken) folder((spokenRoot(s.kind) + "/" + s.folder).trimEnd('/')).songs += s
        val last = setOf(PODCASTS, AUDIOBOOKS)
        for (f in map.values) {
            // The phone's songs first, before the NAS folders; podcasts and audiobooks last.
            f.children.replaceWith(f.children.sortedNatural(compareBy({ it.path != LocalSongs.PHONE_ROOT }, { it.path in last })) { it.name })
            f.songs.replaceWith(f.songs.sortedNatural(TRACK) { it.path })
        }
        folder("")
        map
    }

    /** The newest [VIEW_SIZE] songs: a selection, never a sort of the whole library. */
    val recentlyAdded: List<Song> by lazy {
        val heap = java.util.PriorityQueue<Song>(VIEW_SIZE + 1, compareBy { it.addedAt })
        for (s in songs) {
            heap += s
            if (heap.size > VIEW_SIZE) heap.poll()
        }
        heap.sortedByDescending { it.addedAt }
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

    /**
     * Builds the groupings and the song index now, on the caller's thread,
     * so the first tab opened and the first keystroke only read them. The
     * rest of the indexes are small, or rarely reached.
     */
    fun warm() {
        albums
        artists
        genres
        folders
        books
        shows
        songIndex
    }

    // Built once, with the catalog, so a keystroke folds nothing (docs/plans/029_search.md).
    val songIndex by lazy { SearchIndex.songs(songs) }
    val albumIndex by lazy { SearchIndex(albums, listOf(Field<Album>("album", 0) { it.name }, Field("artist", 1) { it.artist })) }
    val artistIndex by lazy { SearchIndex.names(artists, "artist") { it.name } }
    val genreIndex by lazy { SearchIndex.names(genres, "genre") { it.name } }
    val folderIndex by lazy {
        SearchIndex.names(folders.values.filter { it.path.isNotEmpty() }.sortedNatural { it.path }, "folder") { it.name }
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
        return map.map { (k, list) -> Group(names.getValue(k), list) }.sortedNatural { it.name }
    }

    companion object {
        const val PODCASTS = "Podcasts"
        const val AUDIOBOOKS = "Audiobooks"

        /** How long an automatic view is (docs/plans/010_special_playlists.md). */
        const val VIEW_SIZE = 100

        /** Disc, then track: an album's or a folder's play order, before the natural order of the name. */
        private val TRACK: Comparator<Song> = compareBy({ it.disc }, { it.track })

        private fun spokenRoot(kind: String) = if (kind == Song.PODCAST) PODCASTS else AUDIOBOOKS

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
                    val items = list.sortedNatural(compareBy { it.groupIndex }) { it.path }
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
                }.sortedNatural { it.title }

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

    /** This folder's songs and every subfolder's, in folder order: what "play folder" plays. Built once asked for. */
    val all: List<Song> by lazy { songs + children.flatMap { it.all } }

    /** What the whole subtree holds, for the row's caption: counted once, not flattened on every draw. */
    val songCount: Int by lazy { songs.size + children.sumOf { it.songCount } }
    val durationMs: Long by lazy { songs.sumOf { it.durationMs } + children.sumOf { it.durationMs } }
}

/**
 * Natural, case-blind order, "Track 2" before "Track 10", as a key: lower
 * case, each run of digits stripped of leading zeros and padded to [DIGITS]
 * places. Plain string comparison of two keys then orders as a natural
 * comparator would, and the key is computed once per item rather than on
 * each of the n·log n comparisons of a sort (docs/plans/036_fast_at_100k.md).
 */
fun sortKey(s: String): String {
    val b = StringBuilder(s.length + 8)
    var i = 0
    while (i < s.length) {
        val c = s[i]
        if (c.isDigit()) {
            var j = i
            while (j < s.length && s[j].isDigit()) j++
            var start = i
            while (start < j - 1 && s[start] == '0') start++
            repeat(DIGITS - (j - start)) { b.append('0') }
            b.append(s, start, j)
            i = j
        } else {
            b.append(c.lowercaseChar())
            i++
        }
    }
    return b.toString()
}

/** Digits a number may have and still sort as a number; a longer one sorts as text. */
private const val DIGITS = 12

/** [this] by [first], then by the natural order of [key]; the key is computed once per item. Stable. */
fun <T> Collection<T>.sortedNatural(
    first: Comparator<in T>? = null,
    key: (T) -> String,
): List<T> {
    val items = toList()
    val keys = Array(items.size) { sortKey(key(items[it])) }
    var order: Comparator<Int> = compareBy { keys[it] }
    if (first != null) order = Comparator<Int> { a, b -> first.compare(items[a], items[b]) }.then(order)
    return items.indices.sortedWith(order).map { items[it] }
}

private fun <T> MutableList<T>.replaceWith(sorted: List<T>) {
    clear()
    addAll(sorted)
}
