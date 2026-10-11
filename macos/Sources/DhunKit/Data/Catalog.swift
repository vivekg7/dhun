import Foundation

/// The library as the sidebar shows it, derived from the song list in
/// memory, as on the phone: about 7,000 songs group in a few milliseconds,
/// simpler and faster than keeping albums, artists and folders as tables.
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
    public let artists: [Group]
    public let genres: [Group]
    public let folders: [String: Folder]
    /// Audiobooks and podcast shows as the server grouped them, each one's files in its play order.
    public let books: [Shelf]
    public let shows: [Shelf]
    public let shelves: [String: Shelf]

    public init(_ all: [Song]) {
        let songs = all.filter { !$0.spoken }.sorted { natural($0.title, $1.title) < 0 }
        self.songs = songs
        byId = Dictionary(all.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        albums = Catalog.albums(songs)
        artists = Catalog.group(songs) { $0.artists.isEmpty ? [$0.displayArtist] : $0.artists }
        genres = Catalog.group(songs) { $0.genres.isEmpty ? ["Unknown genre"] : $0.genres }
        folders = Catalog.folders(songs)
        books = Catalog.shelves(all, Song.audiobook)
        shows = Catalog.shelves(all, Song.podcast)
        shelves = Dictionary((books + shows).map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
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
        Dictionary(grouping: all.filter { $0.kind == kind }, by: shelfKey).map { key, list in
            let items = list.sorted {
                $0.groupIndex != $1.groupIndex ? $0.groupIndex < $1.groupIndex : natural($0.path, $1.path) < 0
            }
            let first = items[0]
            let title =
                first.groupTitle.isEmpty
                ? String(first.folder.split(separator: "/").last ?? "") : first.groupTitle
            let authors = items.map { $0.artist.isEmpty ? $0.albumArtist : $0.artist }.filter { !$0.isEmpty }
            return Shelf(id: key, kind: kind, title: title, author: mostCommon(authors), items: items)
        }.sorted { natural($0.title, $1.title) < 0 }
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
        return groups.map { key, list in
            let sorted = list.sorted(by: Catalog.trackOrder)
            let first = sorted[0]
            let artist =
                first.albumArtist.isEmpty ? mostCommon(sorted.map(\.displayArtist)) : first.albumArtist
            return Album(
                id: key, name: first.album, artist: artist, year: sorted.map(\.year).max() ?? 0, songs: sorted
            )
        }.sorted { natural($0.name, $1.name) < 0 }
    }

    /// Every folder, keyed by its path relative to the Music folder ("" is the top).
    private static func folders(_ songs: [Song]) -> [String: Folder] {
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
        for f in map.values {
            f.children.sort { natural($0.name, $1.name) < 0 }
            f.songs.sort { a, b in
                (a.disc, a.track) != (b.disc, b.track)
                    ? (a.disc, a.track) < (b.disc, b.track) : natural(a.path, b.path) < 0
            }
        }
        _ = folder("")
        return map
    }

    public func songsOf(_ ids: [Int]) -> [Song] { ids.compactMap { byId[$0] } }

    // Search indexes (plan 029), each built on first use: most catalogs are never searched.
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
            let all = folders.values.filter { !$0.path.isEmpty }.sorted { natural($0.path, $1.path) < 0 }
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
        return map.map { Group(name: names[$0.key]!, songs: $0.value) }.sorted {
            natural($0.name, $1.name) < 0
        }
    }

    static func trackOrder(_ a: Song, _ b: Song) -> Bool {
        (a.disc, a.track) != (b.disc, b.track)
            ? (a.disc, a.track) < (b.disc, b.track) : natural(a.title, b.title) < 0
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
    init(path: String) { self.path = path }
    public var id: String { path }
    public var name: String {
        guard let i = path.lastIndex(of: "/") else { return path.isEmpty ? "Music" : path }
        return String(path[path.index(after: i)...])
    }
    /// This folder's songs and every subfolder's, in folder order: what "play folder" plays.
    public func allSongs() -> [Song] { songs + children.flatMap { $0.allSongs() } }
}

/// Natural, case-blind order: "Track 2" before "Track 10".
public func natural(_ a: String, _ b: String) -> Int {
    let a = Array(a.unicodeScalars)
    let b = Array(b.unicodeScalars)
    var i = 0
    var j = 0
    func digit(_ c: Unicode.Scalar) -> Bool { c.properties.numericType == .decimal }
    while i < a.count && j < b.count {
        if digit(a[i]) && digit(b[j]) {
            let si = i
            let sj = j
            while i < a.count && digit(a[i]) { i += 1 }
            while j < b.count && digit(b[j]) { j += 1 }
            let na = String(String.UnicodeScalarView(a[si..<i])).drop { $0 == "0" }
            let nb = String(String.UnicodeScalarView(b[sj..<j])).drop { $0 == "0" }
            if na.count != nb.count { return na.count - nb.count }
            if na != nb { return na < nb ? -1 : 1 }
        } else {
            let ca = String(a[i]).lowercased()
            let cb = String(b[j]).lowercased()
            if ca != cb { return ca < cb ? -1 : 1 }
            i += 1
            j += 1
        }
    }
    return (a.count - i) - (b.count - j)
}

private func mostCommon(_ names: [String]) -> String {
    Dictionary(grouping: names, by: { $0 }).max { $0.value.count < $1.value.count }?.key ?? ""
}
