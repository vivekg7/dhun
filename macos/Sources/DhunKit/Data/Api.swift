import Foundation

/// The client for api/openapi.yaml, hand-written as the phone's is (plan 006).
public final class Api: @unchecked Sendable {
    let prefs: Prefs
    let session: URLSession
    /// Every request to our server reports whether it got through, a song's
    /// bytes included, so one that works brings the app back at once (plan 019).
    public var reachable: (@Sendable (Bool) -> Void)?

    public init(prefs: Prefs) {
        self.prefs = prefs
        let c = URLSessionConfiguration.default
        c.timeoutIntervalForRequest = 30
        c.httpAdditionalHeaders = ["Accept-Encoding": "gzip"]
        c.requestCachePolicy = .reloadIgnoringLocalCacheData
        session = URLSession(configuration: c)
    }

    public var server: String { prefs.server }
    public func streamURL(_ song: Int) -> URL { URL(string: "\(server)/api/v1/stream/\(song)")! }
    public func artURL(_ song: Int, size: Int) -> URL {
        URL(string: "\(server)/api/v1/art/\(song)?size=\(size)")!
    }

    public func login(server: String, user: String, password: String, device: String) async throws -> Login {
        try await call(
            "POST", "\(server)/api/v1/login",
            body: ["username": .string(user), "password": .string(password), "device": .string(device)],
            auth: false)
    }

    public func library(since: Int) async throws -> LibraryDTO {
        try await call("GET", "/api/v1/library?since=\(since)")
    }

    public func sync(since: Int, ops: [JSON]) async throws -> SyncState {
        try await call("POST", "/api/v1/sync", body: ["since": JSON(since), "ops": .array(ops)])
    }

    public func thumbs(_ keys: [String]) async throws -> [String: String] {
        let r: ThumbsDTO = try await call(
            "POST", "/api/v1/thumbs", body: ["keys": .array(keys.map { .string($0) })])
        return r.thumbs ?? [:]
    }

    public func lyrics(_ song: Int) async throws -> LyricsDTO {
        try await call("GET", "/api/v1/lyrics/\(song)")
    }
    public func me() async throws -> Me { try await call("GET", "/api/v1/me") }
    public func nowPlaying() async throws -> NowPlaying? {
        let r: NowPlayingDTO = try await call("GET", "/api/v1/now-playing")
        return r.nowPlaying
    }
    public func plays() async throws -> [PlayDTO] {
        let r: PlaysDTO = try await call("GET", "/api/v1/plays")
        return r.plays ?? []
    }
    public func logout() async throws { let _: Empty = try await call("POST", "/api/v1/logout") }

    // Family members, for the admin (plan 023).
    public func members() async throws -> [Member] {
        let r: MembersDTO = try await call("GET", "/api/v1/admin/users")
        return r.users ?? []
    }
    public func addMember(name: String, password: String) async throws {
        let _: Empty = try await call(
            "POST", "/api/v1/admin/users", body: ["name": .string(name), "password": .string(password)])
    }
    public func resetPassword(user: Int, password: String) async throws {
        let _: Empty = try await call(
            "PUT", "/api/v1/admin/users/\(user)/password", body: ["password": .string(password)])
    }

    /// A request with the token, sent only to our own server.
    public func request(_ url: URL) -> URLRequest {
        var r = URLRequest(url: url)
        let token = prefs.token
        if !token.isEmpty, url.absoluteString.hasPrefix(server) {
            r.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        return r
    }

    /// Bytes of a URL on our server (covers), or nil on any failure.
    public func data(_ url: URL) async -> Data? {
        guard let (d, resp) = try? await session.data(for: request(url)) else {
            reachable?(false)
            return nil
        }
        reachable?(true)
        return (resp as? HTTPURLResponse)?.statusCode == 200 ? d : nil
    }

    private func call<T: Decodable>(
        _ method: String, _ path: String, body: [String: JSON]? = nil, auth: Bool = true
    )
        async throws -> T
    {
        let url = URL(string: path.hasPrefix("http") ? path : server + path)!
        var req = auth ? request(url) : URLRequest(url: url)
        req.httpMethod = method
        if let body {
            req.httpBody = Data(JSON.object(body).text.utf8)
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        let data: Data
        let resp: URLResponse
        do {
            (data, resp) = try await session.data(for: req)
        } catch {
            if auth, (error as? URLError)?.code != .cancelled { reachable?(false) }
            throw error
        }
        let http = resp as? HTTPURLResponse
        let status = http?.statusCode ?? 0
        if auth {
            reachable?(true)
            // The server's release, for Settings; sent only to a signed-in device.
            if let v = http?.value(forHTTPHeaderField: "Dhun-Version"), v != prefs.serverVersion {
                prefs.serverVersion = v
            }
        }
        guard (200..<300).contains(status) else {
            let msg = (try? JSONDecoder().decode(ApiErrorDTO.self, from: data))?.message
            throw ApiError(code: status, message: msg ?? "HTTP \(status)")
        }
        if T.self == Empty.self { return Empty() as! T }
        return try JSONDecoder().decode(T.self, from: data)
    }
}

public struct ApiError: Error, CustomStringConvertible {
    public let code: Int
    public let message: String
    public var description: String { message }
}

struct Empty: Decodable {}
struct ApiErrorDTO: Decodable { var code: String?; var message: String? }

public struct Login: Decodable, Sendable {
    public var token: String
    public var deviceId: Int?
    public var user: User?
}

public struct Me: Decodable, Sendable {
    public var user: User?
    public var deviceId: Int?
}

public struct SongDTO: Decodable, Sendable {
    var id: Int
    var path: String?
    var title: String?
    var artist: String?
    var artists: [String]?
    var album: String?
    var albumArtist: String?
    var composer: String?
    var genres: [String]?
    var year: Int?
    var track: Int?
    var disc: Int?
    var durationMs: Int?
    var format: String?
    var bitrate: Int?
    var sampleRate: Int?
    var bitDepth: Int?
    var size: Int?
    var hasArt: Bool?
    var art: String?
    var hasLyrics: Bool?
    var addedAt: String?
    var missing: Bool?

    var song: Song {
        let p = path ?? ""
        let name = ((p as NSString).lastPathComponent as NSString).deletingPathExtension
        return Song(
            id: id, path: p, title: (title ?? "").isEmpty ? name : title!, artist: artist ?? "",
            artists: artists ?? [], album: album ?? "", albumArtist: albumArtist ?? "",
            composer: composer ?? "",
            genres: genres ?? [], year: year ?? 0, track: track ?? 0, disc: disc ?? 0,
            durationMs: durationMs ?? 0,
            format: format ?? "", bitrate: bitrate ?? 0, sampleRate: sampleRate ?? 0, bitDepth: bitDepth ?? 0,
            size: size ?? 0, hasArt: hasArt ?? false, hasLyrics: hasLyrics ?? false,
            addedAt: parseTime(addedAt),
            missing: missing ?? false, art: art ?? "")
    }
}

public struct PlaylistDTO: Decodable, Sendable {
    var id: Int
    var deleted: Bool?
    var path: String?
    var name: String?
    var shared: Bool?
    var ref: String?
    var songs: [Int]?
}

public struct LibraryDTO: Decodable, Sendable {
    var version: Int?
    var songs: [SongDTO]?
    var playlists: [PlaylistDTO]?
}

public struct QueueDTO: Decodable, Sendable {
    var id: String
    var deleted: Bool?
    var name: String?
    var songs: [Int]?
    var currentSong: Int?
    var positionMs: Int?
    var shuffle: Bool?
    var `repeat`: String?
    var usedAt: String?
}

public struct ListItemDTO: Decodable, Sendable {
    var song: Int
    var deleted: Bool?
    var at: String?
}

public struct ResumeDTO: Decodable, Sendable {
    var song: Int
    var deleted: Bool?
    var positionMs: Int?
    var at: String?
}

public struct OpResult: Decodable, Sendable {
    var id: String
    var status: String
    var error: String?
}

public struct SyncState: Decodable, Sendable {
    var version: Int?
    var queues: [QueueDTO]?
    var favorites: [ListItemDTO]?
    var listenLater: [ListItemDTO]?
    var resume: [ResumeDTO]?
    var settings: [String: JSON]?
    var nowPlaying: NowPlaying?
    var results: [OpResult]?
}

public struct PlayDTO: Decodable, Sendable {
    var song: Int
    var count: Int?
    var lastPlayedAt: String?
}

struct PlaysDTO: Decodable { var plays: [PlayDTO]? }
struct ThumbsDTO: Decodable { var thumbs: [String: String]? }
struct NowPlayingDTO: Decodable { var nowPlaying: NowPlaying? }
struct MembersDTO: Decodable { var users: [Member]? }

public struct LyricsDTO: Decodable, Sendable {
    public var source: String?
    public var synced: Bool?
    public var text: String?
}
