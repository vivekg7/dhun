import Foundation

/// A song from the library (api/openapi.yaml `Song`), as the working copy keeps it.
public struct Song: Identifiable, Hashable, Sendable {
    public var id: Int
    public var path: String
    public var title: String
    public var artist: String
    public var artists: [String]
    public var album: String
    public var albumArtist: String
    public var composer: String
    public var genres: [String]
    public var year: Int
    public var track: Int
    public var disc: Int
    public var durationMs: Int
    public var format: String
    public var bitrate: Int
    public var sampleRate: Int
    public var bitDepth: Int
    public var size: Int
    public var hasArt: Bool
    public var hasLyrics: Bool
    public var addedAt: Int
    public var missing: Bool
    /// Which cover it shows, shared by songs showing the same one (plan 019).
    public var art: String
    /// "" for music, else `podcast` or `audiobook` (plan 031).
    public var kind = ""
    /// The release date in full, for an episode.
    public var date = ""
    public var notes = ""
    public var chapters: [Chapter] = []
    /// The book or show it belongs to, as the server grouped them, and its place there.
    public var groupKey = ""
    public var groupTitle = ""
    public var groupIndex = 0

    public static let podcast = "podcast"
    public static let audiobook = "audiobook"

    /// A podcast episode or an audiobook's file: kept out of music lists and playlists, marked played instead of counted.
    public var spoken: Bool { !kind.isEmpty }

    /// An episode's date tag as a day. Some tools write 20251230 for 2025-12-30.
    public var day: String {
        if date.count == 8, date.allSatisfy(\.isASCII), date.allSatisfy(\.isNumber) {
            return "\(date.prefix(4))-\(date.dropFirst(4).prefix(2))-\(date.suffix(2))"
        }
        return String(date.prefix(10))
    }

    public var folder: String {
        guard let i = path.lastIndex(of: "/") else { return "" }
        return String(path[..<i])
    }
    public var displayArtist: String {
        !artist.isEmpty ? artist : !albumArtist.isEmpty ? albumArtist : "Unknown artist"
    }
    public var fileExtension: String { (path as NSString).pathExtension.lowercased() }
}

/// Where a chapter of an episode or a book's file starts.
public struct Chapter: Codable, Hashable, Sendable {
    public var startMs = 0
    public var title = ""

    public init(startMs: Int, title: String) {
        self.startMs = startMs
        self.title = title
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        startMs = try c.decodeIfPresent(Int.self, forKey: .startMs) ?? 0
        title = try c.decodeIfPresent(String.self, forKey: .title) ?? ""
    }

    /// Its title, or "" when that is only its number ("001" in Dune), which the app shows anyway.
    public var name: String { title.allSatisfy(\.isNumber) ? "" : title }
}

/// A playlist as the server indexed it. One made here has a negative id
/// until the server's comes back with the same `ref` (plan 013).
public struct Playlist: Identifiable, Hashable, Sendable {
    public var id: Int
    public var name: String
    public var path: String
    public var shared: Bool
    public var songs: [Int]
    public var ref: String

    /// How ops name it: the server's id, or the ref while the server has not answered.
    public var target: String { id > 0 ? String(id) : "ref:\(ref)" }
}

public struct QueueRow: Identifiable, Hashable, Sendable {
    public var id: String
    public var name: String
    public var songs: [Int]
    public var currentSong: Int
    public var positionMs: Int
    public var shuffle: Bool
    public var repeatMode: String  // off, queue, song
    public var usedAt: Int

    public init(
        id: String, name: String, songs: [Int], currentSong: Int, positionMs: Int, shuffle: Bool,
        repeatMode: String, usedAt: Int
    ) {
        self.id = id
        self.name = name
        self.songs = songs
        self.currentSong = currentSong
        self.positionMs = positionMs
        self.shuffle = shuffle
        self.repeatMode = repeatMode
        self.usedAt = usedAt
    }
}

/// One entry of Favorites (`fav`), Listen Later (`later`) or the played marks
/// (`played`); `at` decides between two edits.
public struct Mark: Hashable, Sendable {
    public var kind: String
    public var song: Int
    public var at: Int
    public var deleted: Bool
}

public struct Resume: Hashable, Sendable {
    public var song: Int
    public var positionMs: Int
    public var at: Int
    public var deleted: Bool
}

/// A place marked in a long file, with an optional note (plan 033). The id
/// is made here, so one made offline has it at once. A deleted one is kept,
/// so an older copy from the server never brings it back.
public struct Bookmark: Hashable, Sendable, Identifiable {
    public var id: String
    public var song: Int
    public var positionMs: Int
    public var note: String
    public var at: Int
    public var deleted: Bool

    public init(id: String, song: Int, positionMs: Int, note: String, at: Int, deleted: Bool) {
        self.id = id
        self.song = song
        self.positionMs = positionMs
        self.note = note
        self.at = at
        self.deleted = deleted
    }

    /// The server's limit on a note.
    public static let maxNote = 1000
}

/// A change not yet acknowledged by the server, as the JSON op it sends.
/// `key` groups ops where only the latest matters.
public struct OutboxOp: Sendable {
    public var id: String
    public var key: String
    public var json: String
}

public struct PlayStat: Hashable, Sendable {
    public var song: Int
    public var count: Int
    public var lastPlayedAt: Int
}

/// Something downloaded, kept in step with the server (plan 012): `kind` is
/// song, album, folder, artist, genre, playlist, list or shelf (a book or a show).
public struct Pin: Hashable, Sendable, Identifiable {
    public var key: String
    public var kind: String
    public var ref: String
    public var name: String
    public var at: Int
    public var id: String { key }
}

public struct Download: Hashable, Sendable {
    public var song: Int
    public var path: String
    public var size: Int
    public var at: Int
}

public struct NowPlaying: Codable, Hashable, Sendable {
    public var deviceId: Int = 0
    public var deviceName: String = ""
    public var queue: String = ""
    public var song: Int = 0
    public var positionMs: Int = 0
    public var playing: Bool = false
    public var at: String = ""
}

public struct User: Codable, Hashable, Sendable {
    public var id: Int = 0
    public var name: String = ""
    public var admin: Bool = false
}

public struct Member: Codable, Hashable, Sendable, Identifiable {
    public var id: Int = 0
    public var name: String = ""
    public var admin: Bool = false
    public var devices: Int = 0
    public var lastSeenAt: String = ""
}

// Every field may be missing in what the server sends (`explicitNulls = false` on the phone).

extension NowPlaying {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        deviceId = try c.decodeIfPresent(Int.self, forKey: .deviceId) ?? 0
        deviceName = try c.decodeIfPresent(String.self, forKey: .deviceName) ?? ""
        queue = try c.decodeIfPresent(String.self, forKey: .queue) ?? ""
        song = try c.decodeIfPresent(Int.self, forKey: .song) ?? 0
        positionMs = try c.decodeIfPresent(Int.self, forKey: .positionMs) ?? 0
        playing = try c.decodeIfPresent(Bool.self, forKey: .playing) ?? false
        at = try c.decodeIfPresent(String.self, forKey: .at) ?? ""
    }
}

extension Member {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decodeIfPresent(Int.self, forKey: .id) ?? 0
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        admin = try c.decodeIfPresent(Bool.self, forKey: .admin) ?? false
        devices = try c.decodeIfPresent(Int.self, forKey: .devices) ?? 0
        lastSeenAt = try c.decodeIfPresent(String.self, forKey: .lastSeenAt) ?? ""
    }
}

extension User {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decodeIfPresent(Int.self, forKey: .id) ?? 0
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        admin = try c.decodeIfPresent(Bool.self, forKey: .admin) ?? false
    }
}
