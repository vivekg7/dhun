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

    public var folder: String {
        guard let i = path.lastIndex(of: "/") else { return "" }
        return String(path[..<i])
    }
    public var displayArtist: String {
        !artist.isEmpty ? artist : !albumArtist.isEmpty ? albumArtist : "Unknown artist"
    }
    public var fileExtension: String { (path as NSString).pathExtension.lowercased() }
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

/// One entry of Favorites (`fav`) or Listen Later (`later`); `at` decides between two edits.
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
/// song, album, folder, artist, genre, playlist or list.
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
