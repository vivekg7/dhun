import Foundation
import Observation

/// The app's long-lived parts and the state the windows show, the phone's
/// `App` (plan 024). Views read these properties; every change goes through
/// `Store` (local write plus outbox op) and the affected list is reloaded
/// from the database, as Room's flows do on the phone.
@MainActor @Observable
public final class AppModel {
    public let prefs: Prefs
    @ObservationIgnored public let db: DB
    @ObservationIgnored public let api: Api
    // Made in init once `self` exists; each is @Observable on its own where it has state.
    @ObservationIgnored private var _sync: Sync!
    @ObservationIgnored private var _playback: Playback!
    @ObservationIgnored private var _covers: Covers!
    @ObservationIgnored private var _lyrics: Lyrics!
    @ObservationIgnored private var _downloads: Downloads!
    @ObservationIgnored private var _cache: SongCache!
    public var sync: Sync { _sync }
    public var playback: Playback { _playback }
    public var covers: Covers { _covers }
    public var lyrics: Lyrics { _lyrics }
    public var downloads: Downloads { _downloads }
    public var cache: SongCache { _cache }
    @ObservationIgnored public let files: URL

    public private(set) var catalog = Catalog.empty
    public private(set) var playlists: [Playlist] = []
    public private(set) var queues: [QueueRow] = []
    public private(set) var favorites: [Mark] = []
    public private(set) var listenLater: [Mark] = []
    public private(set) var resumes: [Resume] = []
    public private(set) var settings: [String: JSON] = [:]
    public private(set) var playStats: [Int: PlayStat] = [:]
    public private(set) var pins: [Pin] = []
    public private(set) var downloaded: [Int: Download] = [:]

    /// False while the server cannot be reached; songs not on this Mac are dimmed then.
    public internal(set) var reachable = true
    /// Signed in with a token. A revoked one keeps everything, outbox included (plan 023).
    public internal(set) var signedIn: Bool
    public internal(set) var revoked = false
    public internal(set) var syncError: String?
    public internal(set) var syncing = false
    /// Bumped when thumbnails arrive, so lists showing placeholders draw again.
    public internal(set) var thumbsArrived = 0
    /// Another device's playback, for hand-off (plan 017).
    public internal(set) var nowPlaying: NowPlaying?
    /// The window's look, this Mac's own (plan 024).
    public var appearance: Appearance {
        didSet {
            prefs.theme = appearance.theme
            prefs.palette = appearance.palette
        }
    }
    /// Download and cache limits, this Mac's own (plans 012, 019).
    public var storage: Storage {
        didSet {
            prefs.downloadLimitGb = storage.downloadLimitGb
            prefs.cacheLimitGb = storage.cacheLimitGb
            prefs.downloadOnExpensive = storage.downloadOnExpensive
            downloads.poke()
            cache.poke()
        }
    }
    public var user: String { prefs.user }
    public var admin: Bool { prefs.admin }

    /// `files` is the app's folder: `~/Library/Application Support/Dhun`.
    public init(files: URL, defaults: UserDefaults = .standard) throws {
        self.files = files
        try FileManager.default.createDirectory(at: files, withIntermediateDirectories: true)
        prefs = Prefs(defaults: defaults, tokenFile: files.appendingPathComponent("token"))
        db = try DB(path: files.appendingPathComponent("dhun.db").path)
        api = Api(prefs: prefs)
        signedIn = !prefs.token.isEmpty
        appearance = Appearance(theme: prefs.theme, palette: prefs.palette)
        storage = Storage(
            downloadLimitGb: prefs.downloadLimitGb, cacheLimitGb: prefs.cacheLimitGb,
            downloadOnExpensive: prefs.downloadOnExpensive)
        _sync = Sync(app: self)
        _covers = Covers(app: self)
        _lyrics = Lyrics(app: self)
        _cache = SongCache(app: self)
        _downloads = Downloads(app: self)
        _playback = Playback(app: self)
        api.reachable = { [weak self] ok in
            Task { @MainActor in
                guard let self, self.reachable != ok else { return }
                self.reachable = ok
                if ok { self.sync.soon() }
            }
        }
        // Signing out forgets the user; a refused token keeps it, so after a
        // relaunch the sign-in still says this Mac's edits were kept.
        revoked = !signedIn && !prefs.user.isEmpty
        reloadAll()
        Task { await reloadSongs() }
    }

    // MARK: Reloads, after a write

    public func reloadAll() {
        reloadPlaylists()
        reloadQueues()
        reloadMarks()
        reloadResumes()
        reloadSettings()
        reloadStats()
        reloadPins()
        reloadDownloads()
    }

    /// The catalogue is grouped off the main thread: 7,000 songs take a few milliseconds there, but not nothing.
    public func reloadSongs() async {
        let db = self.db
        let c = await Task.detached { Catalog(db.songs()) }.value
        catalog = c
        playback.catalogChanged()
    }

    public func reloadPlaylists() { playlists = db.playlists() }
    public func reloadQueues() { queues = db.queues() }
    public func reloadMarks() {
        favorites = db.marks(Store.fav)
        listenLater = db.marks(Store.later)
    }
    public func reloadResumes() { resumes = db.resumes() }
    public func reloadSettings() { settings = db.settings() }
    public func reloadStats() { playStats = db.playStats() }
    public func reloadPins() { pins = db.pins() }
    public func reloadDownloads() { downloaded = db.downloads() }

    // MARK: Account

    /// Signs in; another user's data is wiped first, the same user's kept
    /// (a revoked token signing in again keeps its offline edits).
    public func signIn(server typed: String, user: String, password: String) async throws {
        let server = serverURL(typed)
        let device = Host.current().localizedName ?? "Mac"
        let login = try await api.login(server: server, user: user, password: password, device: device)
        if !prefs.user.isEmpty, prefs.user.lowercased() != user.lowercased() { try forget() }
        prefs.server = server
        prefs.user = login.user?.name ?? user
        prefs.admin = login.user?.admin ?? false
        prefs.deviceId = login.deviceId ?? 0
        prefs.token = login.token
        signedIn = true
        revoked = false
        syncError = nil
        await sync.now()
        await playback.checkHandoff()
    }

    /// Signs out: the token goes, and with it this account's data on this Mac.
    public func signOut() async {
        try? await api.logout()
        playback.reset()
        try? forget()
        prefs.token = ""
        signedIn = false
    }

    private func forget() throws {
        playback.reset()
        try db.wipe()
        downloads.removeAll()
        cache.removeAll()
        prefs.forgetAccount()
        reloadAll()
        catalog = .empty
    }

    /// The token was refused: sign in again, but keep everything (plan 023).
    func tokenRevoked() {
        prefs.token = ""
        signedIn = false
        revoked = true
    }

    /// On becoming active or waking: what other devices played, and what changed.
    public func refresh() {
        guard signedIn else { return }
        Task {
            await playback.checkHandoff()
            await sync.now()
        }
    }
}

public struct Appearance: Equatable, Sendable {
    /// system, light or dark.
    public var theme: String
    public var palette: Int
}

public struct Storage: Equatable, Sendable {
    public var downloadLimitGb: Int
    public var cacheLimitGb: Int
    public var downloadOnExpensive: Bool
}
