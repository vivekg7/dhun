import Foundation

/// Every change to the user's synced data goes through here, as on the
/// phone: it is applied to the working copy at once and recorded in the
/// outbox, which `Sync` pushes when the server is reachable. Offline edits
/// are never lost (AGENTS.md).
@MainActor
public struct Store {
    nonisolated public static let fav = "fav"
    nonisolated public static let later = "later"
    /// An episode or a book's file heard (plan 031).
    nonisolated public static let played = "played"
    nonisolated public static let playedPercent = 90

    let app: AppModel
    var db: DB { app.db }

    public func mark(_ kind: String, _ song: Int, _ on: Bool) {
        db.putMark(Mark(kind: kind, song: song, at: nowMs(), deleted: !on))
        let type =
            switch kind {
            case Store.fav: on ? "favorite.set" : "favorite.unset"
            case Store.played: on ? "played.set" : "played.unset"
            default: on ? "listen_later.add" : "listen_later.remove"
            }
        // Only the last toggle of a song matters.
        record(type, key: "\(kind):\(song)", ["song": JSON(song)])
        app.reloadMarks()
    }

    public func setting(_ name: String, _ value: JSON) {
        db.putSetting(name, value)
        record("setting.set", key: "setting:\(name)", ["name": .string(name), "value": value])
        app.reloadSettings()
    }

    /// Unsets every Playback setting on all the user's devices, so each reads
    /// as its default (plan 034): null, not today's default, so a later
    /// default reaches it.
    public func resetPlayback() {
        for name in Synced.playback { setting(name, .null) }
    }

    /// Where a long file was left, or nil once it was heard to the end (plan 009).
    public func resume(_ song: Int, _ positionMs: Int?) {
        db.putResume(Resume(song: song, positionMs: positionMs ?? 0, at: nowMs(), deleted: positionMs == nil))
        if let positionMs {
            record("resume.set", key: "resume:\(song)", ["song": JSON(song), "positionMs": JSON(positionMs)])
        } else {
            record("resume.unset", key: "resume:\(song)", ["song": JSON(song)])
        }
        app.reloadResumes()
    }

    /// Makes or changes a bookmark (plan 033), sent whole each time; only
    /// the latest change of each is worth sending.
    public func bookmark(_ b: Bookmark) {
        var now = b
        now.at = nowMs()
        db.putBookmark(now)
        if b.deleted {
            record("bookmark.unset", key: "bookmark:\(b.id)", ["bookmark": .string(b.id)])
        } else {
            record(
                "bookmark.set", key: "bookmark:\(b.id)",
                [
                    "bookmark": .string(b.id), "song": JSON(b.song), "positionMs": JSON(b.positionMs),
                    "note": .string(b.note),
                ])
        }
        app.reloadBookmarks()
    }

    // MARK: Queues

    /// A local write only: the player's own bookkeeping (usedAt) that needs no op.
    public func putQueue(_ q: QueueRow) {
        db.putQueue(q)
        app.reloadQueues()
    }

    public func createQueue(_ q: QueueRow) {
        put(
            q, "queue.create",
            [
                "name": .string(q.name), "songs": JSON(q.songs), "song": JSON(q.currentSong),
                "positionMs": JSON(q.positionMs),
            ])
    }

    public func replaceQueue(_ q: QueueRow) {
        put(q, "queue.replace", key: "replace:\(q.id)", ["songs": JSON(q.songs)])
    }

    public func renameQueue(_ q: QueueRow) {
        put(q, "queue.rename", key: "rename:\(q.id)", ["name": .string(q.name)])
    }

    public func deleteQueue(_ id: String) {
        db.deleteQueue(id)
        record("queue.delete", queue: id, ["queue": .string(id)])
        app.reloadQueues()
    }

    /// The queue's current song and position; only the latest is worth sending.
    public func setCurrent(_ q: QueueRow) {
        put(
            q, "queue.set_current", key: "current:\(q.id)",
            ["song": JSON(q.currentSong), "positionMs": JSON(q.positionMs)])
    }

    public func setMode(_ q: QueueRow) {
        put(
            q, "queue.set_mode", key: "mode:\(q.id)",
            ["shuffle": .bool(q.shuffle), "repeat": .string(q.repeatMode)])
    }

    public func insertIntoQueue(_ q: QueueRow, _ songs: [Int], after: Int?) {
        var f: [String: JSON] = ["songs": JSON(songs)]
        if let after { f["after"] = JSON(after) }
        put(q, "queue.insert", f)
    }

    public func removeFromQueue(_ q: QueueRow, _ songs: [Int]) {
        put(q, "queue.remove", ["songs": JSON(songs)])
    }

    public func moveInQueue(_ q: QueueRow, song: Int, after: Int) {
        put(q, "queue.move", ["song": JSON(song), "after": JSON(after)])
    }

    /// Writes `q` and records an op on it, its `queue` field added.
    private func put(_ q: QueueRow, _ type: String, key: String? = nil, _ fields: [String: JSON]) {
        db.putQueue(q)
        record(type, key: key, queue: q.id, fields.merging(["queue": .string(q.id)]) { a, _ in a })
        app.reloadQueues()
    }

    // MARK: Playlists (plan 013)

    /// A new playlist of the user's own; a negative id until the server answers with the same ref.
    @discardableResult
    public func createPlaylist(_ name: String, _ songs: [Int]) -> Playlist {
        let p = Playlist(
            id: -Int.random(in: 1..<Int.max), name: name, path: "", shared: false,
            songs: distinct(music(songs)), ref: UUID().uuidString.lowercased())
        db.putPlaylist(p)
        record(
            "playlist.create", playlist: p.id,
            ["ref": .string(p.ref), "name": .string(name), "songs": JSON(p.songs)])
        app.reloadPlaylists()
        return p
    }

    /// Adds `songs` at the end, skipping those already in it; returns how many were skipped.
    @discardableResult
    public func addToPlaylist(_ p: Playlist, _ songs: [Int]) -> Int {
        let have = Set(p.songs)
        let unique = distinct(music(songs))
        let adding = unique.filter { !have.contains($0) }
        if !adding.isEmpty {
            var q = p
            q.songs += adding
            put(q, "playlist.insert", ["songs": JSON(adding)])
        }
        return unique.count - adding.count
    }

    /// Playlists hold music only: each is an `.m3u8` in Music, which cannot
    /// name a file in Podcasts or Audiobooks, and the server refuses them (plan 031).
    public func music(_ songs: [Int]) -> [Int] { songs.filter { app.catalog.byId[$0]?.spoken != true } }

    /// Removes the entry at `index` of the playlist's own list (unmatched entries included).
    public func removeFromPlaylist(_ p: Playlist, at index: Int) {
        guard p.songs.indices.contains(index) else { return }
        let song = p.songs[index]
        let occurrence = p.songs[..<index].filter { $0 == song }.count
        var q = p
        q.songs.remove(at: index)
        put(q, "playlist.remove", ["song": JSON(song), "occurrence": JSON(occurrence)])
    }

    /// Moves the entry at `from` to `to`, both indexes into the playlist's own list.
    public func moveInPlaylist(_ p: Playlist, from: Int, to: Int) {
        var list = p.songs
        guard list.indices.contains(from), list.indices.contains(to), from != to else { return }
        let song = list[from]
        let occurrence = list[..<from].filter { $0 == song }.count
        list.insert(list.remove(at: from), at: to)
        var q = p
        q.songs = list
        let after = to == 0 ? 0 : list[to - 1]
        // An unmatched entry cannot be named as "after" (0 means the start): send the order.
        if to > 0 && after == 0 { return put(q, "playlist.replace", ["songs": JSON(list)]) }
        var f: [String: JSON] = ["song": JSON(song), "occurrence": JSON(occurrence), "after": JSON(after)]
        if to > 0 { f["afterOccurrence"] = JSON(list[..<(to - 1)].filter { $0 == after }.count) }
        put(q, "playlist.move", f)
    }

    /// The whole order at once (a sort or reverse).
    public func replacePlaylist(_ p: Playlist, _ songs: [Int]) {
        var q = p
        q.songs = songs
        put(q, "playlist.replace", ["songs": JSON(songs)])
    }

    public func renamePlaylist(_ p: Playlist, _ name: String) {
        var q = p
        q.name = name
        put(q, "playlist.rename", ["name": .string(name)])
    }

    /// Writes `p` and records an op on it, its `playlist` field added.
    private func put(_ p: Playlist, _ type: String, _ fields: [String: JSON]) {
        db.putPlaylist(p)
        record(type, playlist: p.id, fields.merging(["playlist": .string(p.target)]) { a, _ in a })
        app.reloadPlaylists()
    }

    /// The server keeps a copy of the file; a download of it goes too.
    public func deletePlaylist(_ p: Playlist) {
        db.deletePlaylist(p.id)
        db.deletePin(Downloads.key(Downloads.playlist, String(p.id)))
        record("playlist.delete", playlist: p.id, ["playlist": .string(p.target)])
        app.reloadPlaylists()
        app.reloadPins()
    }

    // MARK: Playback

    /// Feeds /now-playing, for "Continue from …" on another device.
    public func playbackState(queue: String, song: Int, positionMs: Int, playing: Bool) {
        record(
            "playback.state", key: "playback",
            [
                "queue": .string(queue), "song": JSON(song), "positionMs": JSON(positionMs),
                "playing": .bool(playing),
            ])
    }

    /// One listen (plan 008).
    public func play(_ l: Listen) {
        markPlayed(l)
        var f: [String: JSON] = [
            "song": JSON(l.song), "ms": JSON(l.ms), "endedAt": .string(formatTime(l.endedAt)),
            "fromMs": JSON(l.fromMs),
            "toMs": JSON(l.toMs), "end": .string(l.end), "shuffle": .bool(l.shuffle),
            "utcOffset": JSON(l.utcOffset),
        ]
        if !l.source.isEmpty { f["source"] = .string(l.source) }
        if !l.queue.isEmpty { f["queue"] = .string(l.queue) }
        record("play", at: l.startedAt, f)
    }

    /// An episode or a book's file is played once a listen reaches 90% into
    /// it. The server marks it itself from the listen; this shows it at once,
    /// offline too, as of the same time, so the two agree (plan 031).
    private func markPlayed(_ l: Listen) {
        guard let s = app.catalog.byId[l.song], s.spoken, s.durationMs > 0,
            l.toMs * 100 >= s.durationMs * Store.playedPercent,
            db.mark(Store.played, l.song)?.deleted != false
        else { return }
        db.putMark(Mark(kind: Store.played, song: l.song, at: l.endedAt, deleted: false))
        app.reloadMarks()
    }

    /// `key` replaces earlier ops with the same key. `queue` and `playlist`
    /// mark an op as touching that queue or playlist, so `Sync` does not
    /// overwrite it with an older server copy while the op is still unsent.
    private func record(
        _ type: String, key: String? = nil, queue: String? = nil, playlist: Int? = nil, at: Int = nowMs(),
        _ fields: [String: JSON]
    ) {
        let id = UUID().uuidString.lowercased()
        var op = fields
        op["id"] = .string(id)
        op["type"] = .string(type)
        op["at"] = .string(formatTime(at))
        let tag = queue.map { ":\($0)" } ?? playlist.map { ":" + Store.playlistTag($0) } ?? ""
        do {
            try db.addOp(OutboxOp(id: id, key: key ?? id + tag, json: JSON.object(op).text), replacing: key)
        } catch {
            // The working copy has the change and the server will not: say so where sync errors show.
            NSLog("outbox: %@ not recorded: %@", type, "\(error)")
            app.syncError = "A change could not be kept for sending: \(error.localizedDescription)"
        }
        app.sync.soon()
    }

    /// Ends the outbox key of an op on that playlist.
    nonisolated static func playlistTag(_ id: Int) -> String { "pl\(id)" }
}

extension AppModel {
    public var store: Store { Store(app: self) }

    /// A synced setting, or its default while missing or null (plans 009, 034).
    public func setting<V>(_ key: Synced.Key<V>) -> V { key.value(in: settings) }
}

public struct Listen: Codable, Sendable {
    public var song: Int
    public var startedAt: Int
    public var endedAt: Int
    public var ms: Int
    public var fromMs: Int
    public var toMs: Int
    public var end: String
    public var source: String
    public var queue: String
    public var shuffle: Bool
    public var utcOffset: Int
}
