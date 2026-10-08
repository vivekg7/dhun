import Foundation

/// The library as the sidebar shows it, derived from the song list in
/// memory, as on the phone: about 7,000 songs group in a few milliseconds,
/// simpler and faster than keeping albums, artists and folders as tables.
///
/// Built whole, off the main thread, and never changed after: safe to read
/// from the player's queue too.
public final class Catalog: Sendable {
    public let songs: [Song]
    public let byId: [Int: Song]
    public let albums: [Album]
    public let artists: [Group]
    public let genres: [Group]
    public let folders: [String: Folder]

    public init(_ all: [Song]) {
        let songs = all.sorted { natural($0.title, $1.title) < 0 }
        self.songs = songs
        byId = Dictionary(songs.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        albums = Catalog.albums(songs)
        artists = Catalog.group(songs) { $0.artists.isEmpty ? [$0.displayArtist] : $0.artists }
        genres = Catalog.group(songs) { $0.genres.isEmpty ? ["Unknown genre"] : $0.genres }
        folders = Catalog.folders(songs)
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

    /// Title, album and artist, ignoring case and accents; titles that start with the query first.
    public func search(_ query: String) -> [Song] {
        let q = fold(query.trimmingCharacters(in: .whitespaces))
        guard !q.isEmpty else { return [] }
        let hits = songs.filter {
            fold($0.title).contains(q) || fold($0.album).contains(q) || fold($0.artist).contains(q)
        }
        func rank(_ s: Song) -> Int {
            let t = fold(s.title)
            return t.hasPrefix(q) ? 0 : t.contains(q) ? 1 : 2
        }
        return hits.enumerated().sorted { (rank($0.element), $0.offset) < (rank($1.element), $1.offset) }.map(
            \.element)
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

public func fold(_ s: String) -> String {
    s.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil)
}

private func mostCommon(_ names: [String]) -> String {
    Dictionary(grouping: names, by: { $0 }).max { $0.value.count < $1.value.count }?.key ?? ""
}
