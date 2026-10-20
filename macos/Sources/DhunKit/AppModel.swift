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
    // Lazy, because each takes `self`; init touches them in this order. Each is @Observable on its own
    // where it has state.
    @ObservationIgnored public private(set) lazy var sync = Sync(app: self)
    @ObservationIgnored public private(set) lazy var covers = Covers(app: self)
    @ObservationIgnored public private(set) lazy var lyrics = Lyrics(app: self)
    @ObservationIgnored public private(set) lazy var cache = SongCache(app: self)
    @ObservationIgnored public private(set) lazy var downloads = Downloads(app: self)
    @ObservationIgnored public private(set) lazy var playback = Playback(app: self)
    @ObservationIgnored public let files: URL
    /// Which `reloadSongs` is newest: an older build that finishes last must not win.
    @ObservationIgnored private var catalogBuild = 0

    public private(set) var catalog = Catalog.empty
    public private(set) var playlists: [Playlist] = []
    public private(set) var queues: [QueueRow] = []
    public private(set) var favorites: [Mark] = []
    public private(set) var listenLater: [Mark] = []
    /// Episodes and books' files heard (plan 031).
    public private(set) var played: Set<Int> = []
    public private(set) var resumes: [Resume] = []
    /// Every long file's bookmarks, by place (plan 033).
    public private(set) var bookmarks: [Bookmark] = []
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
    /// Appearance or storage back to this Mac's defaults (plan 034). The keys
    /// are forgotten again after `didSet` wrote the defaults back, so a later
    /// change of default reaches them.
    public func resetAppearance() {
        prefs.reset(Prefs.appearanceKeys)
        appearance = Appearance(theme: prefs.theme, palette: prefs.palette)
        prefs.reset(Prefs.appearanceKeys)
    }
    public func resetStorage() {
        prefs.reset(Prefs.storageKeys)
        storage = Storage(
            downloadLimitGb: prefs.downloadLimitGb, cacheLimitGb: prefs.cacheLimitGb,
            downloadOnExpensive: prefs.downloadOnExpensive)
        prefs.reset(Prefs.storageKeys)
    }

    /// The last ten searches, newest first: this Mac's own, as each phone keeps its own (plan 029).
    public private(set) var recentSearches: [String] = []

    /// Kept when something it found is opened or played, not on every keystroke.
    public func keepSearch(_ query: String) {
        let q = query.trimmingCharacters(in: .whitespaces)
        guard !q.isEmpty else { return }
        recentSearches = Array(([q] + recentSearches.filter { $0.lowercased() != q.lowercased() }).prefix(10))
        prefs.recentSearches = recentSearches
    }

    public func forgetSearches() {
        recentSearches = []
        prefs.recentSearches = []
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
        recentSearches = prefs.recentSearches
        storage = Storage(
            downloadLimitGb: prefs.downloadLimitGb, cacheLimitGb: prefs.cacheLimitGb,
            downloadOnExpensive: prefs.downloadOnExpensive)
        _ = (sync, covers, lyrics, cache, downloads, playback)
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
        reloadBookmarks()
        reloadSettings()
        reloadStats()
        reloadPins()
        reloadDownloads()
    }

    /// The catalogue is grouped off the main thread, and the songs' and
    /// albums' search indexes with it: 30,000 songs today, sized for 100,000
    /// (plan 036), so the first keystroke in a filter must find them built.
    public func reloadSongs() async {
        catalogBuild += 1
        let build = catalogBuild
        let db = self.db
        let c = await Task.detached {
            let c = Catalog(db.songs())
            _ = (c.songIndex, c.albumIndex)
            return c
        }.value
        guard build == catalogBuild else { return }
        catalog = c
        playback.catalogChanged()
    }

    public func reloadPlaylists() { playlists = db.playlists() }
    public func reloadQueues() { queues = db.queues() }
    public func reloadMarks() {
        favorites = db.marks(Store.fav)
        listenLater = db.marks(Store.later)
        played = Set(db.marks(Store.played).map(\.song))
    }
    public func reloadResumes() { resumes = db.resumes() }
    public func reloadBookmarks() { bookmarks = db.bookmarks() }
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
