import Foundation
import Network

/// Keeps the working copy and the server in step (plan 006), as the phone's
/// `Sync` does: fetch catalogue changes, push the outbox, pull what changed
/// elsewhere. Runs on becoming active, a couple of seconds after each local
/// change, when the network returns, and on "Sync now".
@MainActor
public final class Sync {
    private unowned let app: AppModel
    private var db: DB { app.db }
    private var prefs: Prefs { app.prefs }
    private var pending: Task<Void, Never>?
    private var retryDelay = Sync.retryMin
    private var running = false
    private var again = false
    private let monitor = NWPathMonitor()
    /// A playlist made here and the server id it became, for a view still showing the old one.
    public private(set) var replaced: [Int: Int] = [:]
    /// The current network costs money (a phone's hotspot): download less ahead (plans 012, 019).
    public private(set) var expensive = false

    // A blip is over in seconds; the 15 minutes this once grew to on the
    // phone left it offline long after the network was back (plan 019).
    static let retryMin: Double = 5
    static let retryMax: Double = 120

    /// Raised when songs gain a field: 1 is the art key (plan 019).
    static let libraryFormat = 1

    init(app: AppModel) {
        self.app = app
        monitor.pathUpdateHandler = { [weak self] path in
            Task { @MainActor in
                guard let self else { return }
                self.expensive = path.isExpensive
                if path.status == .satisfied {
                    // Back online: send what piled up while offline.
                    self.soon()
                    self.app.downloads.poke()
                    self.app.cache.poke()
                } else {
                    self.app.reachable = false
                }
            }
        }
        monitor.start(queue: .global(qos: .utility))
    }

    /// Debounced: a burst of clicks becomes one request. A failed sync
    /// retries with a growing delay while the app runs; the outbox is on disk.
    public func soon(after seconds: Double = 2) {
        pending?.cancel()
        pending = Task { [weak self] in
            try? await Task.sleep(for: .seconds(seconds))
            guard let self, !Task.isCancelled else { return }
            // Its own task: the next edit's debounce cancels only the wait,
            // never a sync under way (its requests would be cancelled with it).
            Task { await self.runRetrying() }
        }
    }

    private func runRetrying() async {
        if await now() {
            retryDelay = Sync.retryMin
        } else {
            retryDelay = min(retryDelay * 2, Sync.retryMax)
            soon(after: retryDelay)
        }
    }

    /// Returns false when the server could not be reached, so `soon` tries again later.
    @discardableResult
    public func now() async -> Bool {
        guard app.signedIn else { return true }
        // One at a time; a request while one runs makes it run once more.
        if running {
            again = true
            return true
        }
        running = true
        app.syncing = true
        defer {
            running = false
            app.syncing = false
        }
        var ok = true
        repeat {
            again = false
            ok = await round()
        } while again && ok
        return ok
    }

    private func round() async -> Bool {
        do {
            // A field added to songs comes only with the songs the server
            // sends again, and it would send none that had not changed.
            if prefs.libraryFormat < Sync.libraryFormat { prefs.libraryVersion = 0 }
            try await pullLibrary()
            prefs.libraryFormat = Sync.libraryFormat
            // More than 500 ops (a long offline stretch) take several rounds.
            pushedPlaylists = false
            while try await pushAndPull() > 0, db.outboxSize() > 0 {}
            // The server rewrote those playlists: fetch them as written.
            if pushedPlaylists { try await pullLibrary() }
            try await pullPlays()
            app.syncError = nil
            Task { await app.covers.fetchThumbs() }
            app.downloads.poke()
            return true
        } catch let e as ApiError {
            // A revoked token (password reset, device removed): sign in again,
            // but keep everything. Signing out would clear the outbox, and
            // with it the edits made offline (plan 023).
            if e.code == 401 { app.tokenRevoked() }
            app.syncError = e.message
            return !(500..<600).contains(e.code)
        } catch {
            app.syncError = error.localizedDescription
            return false
        }
    }

    private var pushedPlaylists = false

    /// A playlist with an op still in the outbox is left as edited here, and
    /// the cursor stays put so the next pull brings it again once the op has
    /// gone (plan 013).
    private func pullLibrary() async throws {
        let lib = try await app.api.library(since: prefs.libraryVersion)
        let songs = (lib.songs ?? []).map(\.song)
        if !songs.isEmpty {
            let db = self.db
            try await Task.detached { try db.putSongs(songs) }.value
        }
        let pending = db.outbox().map(\.key)
        func edited(_ id: Int) -> Bool { pending.contains { $0.hasSuffix(":" + Store.playlistTag(id)) } }
        var skipped = false
        let local = db.localPlaylists()
        for p in lib.playlists ?? [] {
            if p.deleted == true {
                if edited(p.id) { skipped = true } else { db.deletePlaylist(p.id) }
                continue
            }
            let ref = p.ref ?? ""
            // One made here, back from the server with its real id. A server
            // older than `ref` is matched by name, once its create has gone.
            let mine =
                local.first { !$0.ref.isEmpty && $0.ref == ref }
                ?? local.first { ref.isEmpty && p.shared != true && $0.name == p.name && !edited($0.id) }
            if let mine {
                if edited(mine.id) {
                    skipped = true
                    continue
                }
                db.deletePlaylist(mine.id)
                db.movePin(
                    from: Downloads.key(Downloads.playlist, String(mine.id)),
                    to: Downloads.key(Downloads.playlist, String(p.id)), ref: String(p.id))
                replaced[mine.id] = p.id
            } else if edited(p.id) {
                skipped = true
                continue
            }
            db.putPlaylist(
                Playlist(
                    id: p.id, name: p.name ?? "", path: p.path ?? "", shared: p.shared ?? false,
                    songs: p.songs ?? [],
                    ref: ref))
        }
        if !skipped { prefs.libraryVersion = lib.version ?? prefs.libraryVersion }
        app.reloadPlaylists()
        app.reloadPins()
        if !songs.isEmpty { await app.reloadSongs() }
    }

    /// One round trip; returns how many ops the server answered.
    private func pushAndPull() async throws -> Int {
        let ops = db.outbox(limit: 500)
        if ops.contains(where: { $0.json.contains("\"type\":\"playlist.") }) { pushedPlaylists = true }
        let state = try await app.api.sync(since: prefs.syncVersion, ops: ops.map { JSON.parse($0.json) })
        // applied, duplicate and rejected all leave the outbox: a rejected op
        // can never succeed (api/openapi.yaml).
        let results = state.results ?? []
        let done = results.map(\.id)
        // A playlist the server refused to create (a name it reserves) would
        // otherwise stay here forever as a local-only row.
        let refused = Set(results.filter { $0.status == "rejected" }.map(\.id))
        for op in ops where refused.contains(op.id) && op.json.contains("\"type\":\"playlist.create\"") {
            if let r = op.key.range(of: ":pl", options: .backwards), let id = Int(op.key[r.upperBound...]) {
                db.deletePlaylist(id)
            }
        }
        if !done.isEmpty { db.ackOps(done) }
        apply(state)
        prefs.syncVersion = state.version ?? prefs.syncVersion
        return done.count
    }

    /// Applies what the server says changed. Anything with an op still in
    /// the outbox (made while the request was in flight) is left alone: that
    /// op goes with the next sync and the server's answer will include it.
    private func apply(_ s: SyncState) {
        let pending = Set(db.outbox().map(\.key))
        for q in s.queues ?? [] {
            if q.deleted == true {
                db.deleteQueue(q.id)
                continue
            }
            if pending.contains(where: { $0.hasSuffix(":\(q.id)") }) { continue }
            db.putQueue(
                QueueRow(
                    id: q.id, name: q.name ?? "", songs: q.songs ?? [], currentSong: q.currentSong ?? 0,
                    positionMs: q.positionMs ?? 0, shuffle: q.shuffle ?? false, repeatMode: q.repeat ?? "off",
                    usedAt: parseTime(q.usedAt)))
        }
        for (kind, items) in [(Store.fav, s.favorites ?? []), (Store.later, s.listenLater ?? [])] {
            for i in items {
                let at = parseTime(i.at)
                if pending.contains("\(kind):\(i.song)") { continue }
                if let local = db.mark(kind, i.song), local.at > at { continue }
                db.putMark(Mark(kind: kind, song: i.song, at: at, deleted: i.deleted ?? false))
            }
        }
        for r in s.resume ?? [] {
            let at = parseTime(r.at)
            if pending.contains("resume:\(r.song)") { continue }
            if let local = db.resume(r.song), local.at > at { continue }
            db.putResume(
                Resume(song: r.song, positionMs: r.positionMs ?? 0, at: at, deleted: r.deleted ?? false))
        }
        for (name, value) in s.settings ?? [:] where !pending.contains("setting:\(name)") {
            db.putSetting(name, value)
        }
        // Sent only when it changed since the cursor: no news is not "nothing playing".
        if let np = s.nowPlaying { app.nowPlaying = np }
        app.reloadQueues()
        app.reloadMarks()
        app.reloadResumes()
        app.reloadSettings()
        app.playback.queuesSynced()
    }

    private func pullPlays() async throws {
        let plays = try await app.api.plays()
        try db.replacePlayStats(
            plays.map {
                PlayStat(song: $0.song, count: $0.count ?? 0, lastPlayedAt: parseTime($0.lastPlayedAt))
            })
        app.reloadStats()
    }
}
