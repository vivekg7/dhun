import Foundation

/// The library as the sidebar shows it, derived from the song list in
/// memory, as on the phone. About 30,000 songs today, designed for 100,000
/// (plan 036): a whole rebuild off the main thread still beats keeping
/// albums, artists and folders as tables, as long as nothing in it is done
/// once per comparison or once per row drawn.
///
/// Built whole, off the main thread, and never changed after: safe to read
/// from the player's queue too.
public final class Catalog: Sendable {
    /// The music: what every song list, view and search of songs shows.
    /// Podcasts and audiobooks have their own pages (`books`, `shows`), so
    /// an episode never turns up in an artist or a shuffle (plan 031).
    public let songs: [Song]
    /// Every song, episodes and books' files too: what queues, Favorites and Listen Later name.
    public let byId: [Int: Song]
    public let albums: [Album]
    public let albumsById: [String: Album]
    /// Each song's album, for "Go to Album" and an artist's album strip.
    public let albumOf: [Int: String]
    public let artists: [Group]
    public let genres: [Group]
    public let folders: [String: Folder]
    /// Audiobooks and podcast shows as the server grouped them, each one's files in its play order.
    public let books: [Shelf]
    public let shows: [Shelf]
    public let shelves: [String: Shelf]
    /// The newest `viewSize` songs, newest first: the automatic view (plan 010), chosen here rather than
    /// by sorting the whole library on the main thread each time the page draws.
    public let recentlyAdded: [Song]

    /// Each automatic view shows at most this many songs, as on the phone.
    public static let viewSize = 100

    public init(_ all: [Song]) {
        let songs = naturalSorted(all.filter { !$0.spoken }, \.title)
        self.songs = songs
        byId = Dictionary(all.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        albums = Catalog.albums(songs)
        albumsById = Dictionary(albums.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        var albumOf: [Int: String] = [:]
        for a in albums { for s in a.songs { albumOf[s.id] = a.id } }
        self.albumOf = albumOf
        artists = Catalog.group(songs) { $0.artists.isEmpty ? [$0.displayArtist] : $0.artists }
        genres = Catalog.group(songs) { $0.genres.isEmpty ? ["Unknown genre"] : $0.genres }
        folders = Catalog.folders(songs, all.filter(\.spoken))
        books = Catalog.shelves(all, Song.audiobook)
        shows = Catalog.shelves(all, Song.podcast)
        shelves = Dictionary((books + shows).map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        // Indices, not songs: a Song is thirty fields, and the sort moves only Ints.
        recentlyAdded = songs.indices.sorted { songs[$0].addedAt > songs[$1].addedAt }
            .prefix(Catalog.viewSize).map { songs[$0] }
    }

    /// The shelf `s` is on, or nil for music.
    public func shelfOf(_ s: Song) -> Shelf? { s.spoken ? shelves[Catalog.shelfKey(s)] : nil }

    /// The song whose cover `s` shows: an episode or a book's file with no
    /// cover of its own shows its show's or book's.
    public func artOf(_ s: Song) -> Song {
        guard s.spoken, !s.hasArt else { return s }
        return shelfOf(s)?.cover ?? s
    }

    /// A file the server did not group (none should be) is a shelf of its folder.
    private static func shelfKey(_ s: Song) -> String {
        s.groupKey.isEmpty ? s.kind + "\u{0}" + s.folder : s.groupKey
    }

    private static func shelves(_ all: [Song], _ kind: String) -> [Shelf] {
        let shelves = Dictionary(grouping: all.filter { $0.kind == kind }, by: shelfKey).map { key, list in
            let items = pathOrder(list) { ($0.groupIndex, 0) }
            let first = items[0]
            let title =
                first.groupTitle.isEmpty
                ? String(first.folder.split(separator: "/").last ?? "") : first.groupTitle
            let authors = items.map { $0.artist.isEmpty ? $0.albumArtist : $0.artist }.filter { !$0.isEmpty }
            return Shelf(id: key, kind: kind, title: title, author: mostCommon(authors), items: items)
        }
        return naturalSorted(shelves, \.title)
    }

    public static let empty = Catalog([])

    /// Two songs are on one album when they share the album name and either
    /// the album artist or, without one, the folder. A bare album name would
    /// merge every "Greatest Hits" in the collection.
    private static func albums(_ songs: [Song]) -> [Album] {
        let groups = Dictionary(grouping: songs.filter { !$0.album.isEmpty }) {
            $0.album.lowercased() + "\u{0}"
                + ($0.albumArtist.isEmpty ? $0.folder : $0.albumArtist.lowercased())
        }
        let albums = groups.map { key, list in
            let sorted = Catalog.trackOrder(list)
            let first = sorted[0]
            let artist =
                first.albumArtist.isEmpty ? mostCommon(sorted.map(\.displayArtist)) : first.albumArtist
            return Album(
                id: key, name: first.album, artist: artist, year: sorted.map(\.year).max() ?? 0, songs: sorted
            )
        }
        return naturalSorted(albums, \.name)
    }

    public static let podcasts = "Podcasts"
    public static let audiobooks = "Audiobooks"

    /// Every folder, keyed by its path relative to the Music folder ("" is
    /// the top). Episodes and books' files sit under a Podcasts and an
    /// Audiobooks root after the music's folders, as on the phone: their
    /// paths are relative to folders of their own, so a show's folder could
    /// share a music folder's name.
    private static func folders(_ songs: [Song], _ spoken: [Song]) -> [String: Folder] {
        var map: [String: Folder] = [:]
        func folder(_ path: String) -> Folder {
            if let f = map[path] { return f }
            let f = Folder(path: path)
            map[path] = f
            if !path.isEmpty {
                let parent = path.lastIndex(of: "/").map { String(path[..<$0]) } ?? ""
                folder(parent).children.append(f)
            }
            return f
        }
        for s in songs { folder(s.folder).songs.append(s) }
        for s in spoken {
            let root = s.kind == Song.podcast ? podcasts : audiobooks
            folder(s.folder.isEmpty ? root : root + "/" + s.folder).songs.append(s)
        }
        let last: Set = [podcasts, audiobooks]
        for f in map.values {
            let keyed = f.children.map { (last.contains($0.path) ? 1 : 0, sortKey($0.name), $0) }
            f.children = keyed.sorted { ($0.0, $0.1) < ($1.0, $1.1) }.map(\.2)
            f.songs = pathOrder(f.songs) { ($0.disc, $0.track) }
        }
        let root = folder("")
        // Each folder's total, children first, so a row never flattens its subtree to count it.
        func total(_ f: Folder) {
            f.songCount = f.songs.count
            f.durationMs = f.songs.reduce(0) { $0 + $1.durationMs }
            for c in f.children {
                total(c)
                f.songCount += c.songCount
                f.durationMs += c.durationMs
            }
        }
        total(root)
        return map
    }

    public func songsOf(_ ids: [Int]) -> [Song] { ids.compactMap { byId[$0] } }

    // Search indexes (plan 029), each built on first use. `reloadSongs` builds
    // the songs' and albums' with the catalogue, off the main thread, so the
    // first keystroke in a filter finds them ready (plan 036).
    public var songIndex: SearchIndex<Song> { indexes.get(\.songs) { SearchIndex.songs(songs) } }
    public var albumIndex: SearchIndex<Album> {
        indexes.get(\.albums) {
            SearchIndex(
                albums, [SearchField("album", 0) { $0.name }, SearchField("artist", 1) { $0.artist }])
        }
    }
    public var artistIndex: SearchIndex<Group> {
        indexes.get(\.artists) { SearchIndex.names(artists, "artist") { $0.name } }
    }
    public var genreIndex: SearchIndex<Group> {
        indexes.get(\.genres) { SearchIndex.names(genres, "genre") { $0.name } }
    }
    public var folderIndex: SearchIndex<Folder> {
        indexes.get(\.folders) {
            let all = naturalSorted(folders.values.filter { !$0.path.isEmpty }, \.path)
            return SearchIndex.names(all, "folder") { $0.name }
        }
    }

    public var bookIndex: SearchIndex<Shelf> {
        indexes.get(\.books) {
            SearchIndex(books, [SearchField("book", 0) { $0.title }, SearchField("artist", 1) { $0.author }])
        }
    }
    public var showIndex: SearchIndex<Shelf> {
        indexes.get(\.shows) { SearchIndex.names(shows, "show") { $0.title } }
    }
    public var episodeIndex: SearchIndex<Song> {
        indexes.get(\.episodes) { SearchIndex.songs(shows.flatMap(\.items)) }
    }

    /// The songs of `list` that `query` finds, in the list's order: what a page's filter box shows.
    public func filter(_ list: [Song], _ query: String) -> [Song] {
        let q = SearchQuery(query)
        if q.isEmpty { return list }
        // Favorites, Listen Later and queues may hold episodes, which the music's index leaves out.
        let index = list.contains(where: \.spoken) ? SearchIndex.songs(list) : songIndex
        let found = Set(index.filter(q).map(\.id))
        return list.filter { found.contains($0.id) }
    }

    private let indexes = Indexes()

    private final class Indexes: @unchecked Sendable {
        let lock = NSLock()
        var songs: SearchIndex<Song>?
        var albums: SearchIndex<Album>?
        var artists: SearchIndex<Group>?
        var genres: SearchIndex<Group>?
        var folders: SearchIndex<Folder>?
        var books: SearchIndex<Shelf>?
        var shows: SearchIndex<Shelf>?
        var episodes: SearchIndex<Song>?

        func get<T>(_ at: ReferenceWritableKeyPath<Indexes, T?>, _ make: () -> T) -> T {
            lock.withLock {
                if let built = self[keyPath: at] { return built }
                let built = make()
                self[keyPath: at] = built
                return built
            }
        }
    }

    private static func group(_ songs: [Song], _ keys: (Song) -> [String]) -> [Group] {
        var map: [String: [Song]] = [:]
        var names: [String: String] = [:]
        for s in songs {
            for k in keys(s) {
                let lower = k.lowercased()
                if names[lower] == nil { names[lower] = k }
                map[lower, default: []].append(s)
            }
        }
        return naturalSorted(map.map { Group(name: names[$0.key]!, songs: $0.value) }, \.name)
    }

    /// An album's order: disc and track, then the title for untagged files.
    static func trackOrder(_ songs: [Song]) -> [Song] {
        songs.map { ($0.disc, $0.track, sortKey($0.title), $0) }
            .sorted { ($0.0, $0.1, $0.2) < ($1.0, $1.1, $1.2) }.map(\.3)
    }

    /// `rank` first, then the path in natural order: a folder's or a shelf's files.
    private static func pathOrder(_ songs: [Song], _ rank: (Song) -> (Int, Int)) -> [Song] {
        songs.map { (rank($0), sortKey($0.path), $0) }
            .sorted { ($0.0.0, $0.0.1, $0.1) < ($1.0.0, $1.0.1, $1.1) }.map(\.2)
    }
}

public struct Album: Identifiable, Hashable, Sendable {
    public let id: String
    public let name: String
    public let artist: String
    public let year: Int
    public let songs: [Song]
}

public struct Group: Identifiable, Hashable, Sendable {
    public let name: String
    public let songs: [Song]
    public var id: String { name.lowercased() }
}

extension [Group] {
    /// The artist or genre a link or a download names: case does not count.
    public func named(_ name: String) -> Group? {
        let id = name.lowercased()
        return first { $0.id == id }
    }
}

/// An audiobook or a podcast show (plan 031): a book's files in reading
/// order, a show's episodes oldest first.
public struct Shelf: Identifiable, Hashable, Sendable {
    public let id: String
    public let kind: String
    public let title: String
    /// The author, or the host; "" when the tags name none.
    public let author: String
    public let items: [Song]

    public var isBook: Bool { kind == Song.audiobook }

    /// The first file with a cover: the one its files without one show.
    public var cover: Song? { items.first { $0.hasArt } }
}

/// Filled while the catalogue is built, then only read.
public final class Folder: Identifiable, @unchecked Sendable {
    public let path: String
    public var children: [Folder] = []
    public var songs: [Song] = []
    /// This folder's songs and every subfolder's, counted once when the catalogue is built.
    public internal(set) var songCount = 0
    public internal(set) var durationMs = 0
    init(path: String) { self.path = path }
    public var id: String { path }
    public var name: String {
        guard let i = path.lastIndex(of: "/") else { return path.isEmpty ? "Music" : path }
        return String(path[path.index(after: i)...])
    }
    /// This folder's songs and every subfolder's, in folder order: what "play folder" plays.
    public func allSongs() -> [Song] { songs + children.flatMap { $0.allSongs() } }
}

/// `items` in natural, case-blind order of `text`: "Track 2" before "Track 10".
/// The key is made once per item, not once per comparison: a sort of n
/// items compares about n log n times, and a comparator that split both
/// strings each time was half the cost of building the catalogue (plan 036).
public func naturalSorted<T>(_ items: [T], _ text: (T) -> String) -> [T] {
    items.map { (sortKey(text($0)), $0) }.sorted { $0.0 < $1.0 }.map(\.1)
}

/// A string's natural-order key: lower case, each run of digits without
/// its leading zeros and padded to a fixed width, so plain `<` on two keys
/// puts 2 before 10 and ignores case.
public func sortKey(_ s: String) -> String {
    var out = String.UnicodeScalarView()
    var digits: [Unicode.Scalar] = []
    func flush() {
        guard !digits.isEmpty else { return }
        let run = digits.drop { $0 == "0" }
        out.append(contentsOf: repeatElement("0", count: max(0, digitWidth - run.count)))
        out.append(contentsOf: run)
        digits.removeAll(keepingCapacity: true)
    }
    for c in s.lowercased().unicodeScalars {
        if c.properties.numericType == .decimal {
            digits.append(c)
        } else {
            flush()
            out.append(c)
        }
    }
    flush()
    return String(out)
}

/// Longer runs of digits sort by their text, which is wrong past this many; no title has them.
private let digitWidth = 20

private func mostCommon(_ names: [String]) -> String {
    Dictionary(grouping: names, by: { $0 }).max { $0.value.count < $1.value.count }?.key ?? ""
}
