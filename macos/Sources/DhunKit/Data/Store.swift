import Foundation

/// Every change to the user's synced data goes through here, as on the
/// phone: it is applied to the working copy at once and recorded in the
/// outbox, which `Sync` pushes when the server is reachable. Offline edits
/// are never lost (AGENTS.md).
@MainActor
public struct Store {
    nonisolated public static let fav = "fav"
    nonisolated public static let later = "later"

    let app: AppModel
    var db: DB { app.db }

    public func mark(_ kind: String, _ song: Int, _ on: Bool) {
        db.putMark(Mark(kind: kind, song: song, at: nowMs(), deleted: !on))
        let type =
            kind == Store.fav
            ? (on ? "favorite.set" : "favorite.unset") : (on ? "listen_later.add" : "listen_later.remove")
        // Only the last toggle of a song matters.
        record(type, key: "\(kind):\(song)", ["song": JSON(song)])
        app.reloadMarks()
    }

    public func setting(_ name: String, _ value: JSON) {
        db.putSetting(name, value)
        record("setting.set", key: "setting:\(name)", ["name": .string(name), "value": value])
        app.reloadSettings()
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

    // MARK: Queues

    /// A local write only: the player's own bookkeeping (usedAt) that needs no op.
    public func putQueue(_ q: QueueRow) {
        db.putQueue(q)
        app.reloadQueues()
    }

    public func createQueue(_ q: QueueRow) {
        db.putQueue(q)
        record(
            "queue.create", queue: q.id,
            [
                "queue": .string(q.id), "name": .string(q.name), "songs": JSON(q.songs),
                "song": JSON(q.currentSong),
                "positionMs": JSON(q.positionMs),
            ])
        app.reloadQueues()
    }

    public func replaceQueue(_ q: QueueRow) {
        db.putQueue(q)
        record("queue.replace", key: "replace:\(q.id)", ["queue": .string(q.id), "songs": JSON(q.songs)])
        app.reloadQueues()
    }

    public func renameQueue(_ q: QueueRow) {
        db.putQueue(q)
        record("queue.rename", key: "rename:\(q.id)", ["queue": .string(q.id), "name": .string(q.name)])
        app.reloadQueues()
    }

    public func deleteQueue(_ id: String) {
        db.deleteQueue(id)
        record("queue.delete", queue: id, ["queue": .string(id)])
        app.reloadQueues()
    }

    /// The queue's current song and position; only the latest is worth sending.
    public func setCurrent(_ q: QueueRow) {
        db.putQueue(q)
        record(
            "queue.set_current", key: "current:\(q.id)",
            ["queue": .string(q.id), "song": JSON(q.currentSong), "positionMs": JSON(q.positionMs)])
        app.reloadQueues()
    }

    public func setMode(_ q: QueueRow) {
        db.putQueue(q)
        record(
            "queue.set_mode", key: "mode:\(q.id)",
            ["queue": .string(q.id), "shuffle": .bool(q.shuffle), "repeat": .string(q.repeatMode)])
        app.reloadQueues()
    }

    public func insertIntoQueue(_ q: QueueRow, _ songs: [Int], after: Int?) {
        db.putQueue(q)
        var f: [String: JSON] = ["queue": .string(q.id), "songs": JSON(songs)]
        if let after { f["after"] = JSON(after) }
        record("queue.insert", queue: q.id, f)
        app.reloadQueues()
    }

    public func removeFromQueue(_ q: QueueRow, _ songs: [Int]) {
        db.putQueue(q)
        record("queue.remove", queue: q.id, ["queue": .string(q.id), "songs": JSON(songs)])
        app.reloadQueues()
    }

    public func moveInQueue(_ q: QueueRow, song: Int, after: Int) {
        db.putQueue(q)
        record("queue.move", queue: q.id, ["queue": .string(q.id), "song": JSON(song), "after": JSON(after)])
        app.reloadQueues()
    }

    // MARK: Playlists (plan 013)

    /// A new playlist of the user's own; a negative id until the server answers with the same ref.
    @discardableResult
    public func createPlaylist(_ name: String, _ songs: [Int]) -> Playlist {
        var seen = Set<Int>()
        let p = Playlist(
            id: -Int.random(in: 1..<Int.max), name: name, path: "", shared: false,
            songs: songs.filter { seen.insert($0).inserted }, ref: UUID().uuidString.lowercased())
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
        var seen = Set<Int>()
        let distinct = songs.filter { seen.insert($0).inserted }
        let adding = distinct.filter { !have.contains($0) }
        if !adding.isEmpty {
            var q = p
            q.songs += adding
            db.putPlaylist(q)
            record("playlist.insert", playlist: p.id, ["playlist": .string(p.target), "songs": JSON(adding)])
            app.reloadPlaylists()
        }
        return distinct.count - adding.count
    }

    /// Removes the entry at `index` of the playlist's own list (unmatched entries included).
    public func removeFromPlaylist(_ p: Playlist, at index: Int) {
        guard p.songs.indices.contains(index) else { return }
        let song = p.songs[index]
        let occurrence = p.songs[..<index].filter { $0 == song }.count
        var q = p
        q.songs.remove(at: index)
        db.putPlaylist(q)
        record(
            "playlist.remove", playlist: p.id,
            ["playlist": .string(p.target), "song": JSON(song), "occurrence": JSON(occurrence)])
        app.reloadPlaylists()
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
        db.putPlaylist(q)
        defer { app.reloadPlaylists() }
        let after = to == 0 ? 0 : list[to - 1]
        if to > 0 && after == 0 {
            // An unmatched entry cannot be named as "after" (0 means the start): send the order.
            record("playlist.replace", playlist: p.id, ["playlist": .string(p.target), "songs": JSON(list)])
            return
        }
        var f: [String: JSON] = [
            "playlist": .string(p.target), "song": JSON(song), "occurrence": JSON(occurrence),
            "after": JSON(after),
        ]
        if to > 0 { f["afterOccurrence"] = JSON(list[..<(to - 1)].filter { $0 == after }.count) }
        record("playlist.move", playlist: p.id, f)
    }

    /// The whole order at once (a sort or reverse).
    public func replacePlaylist(_ p: Playlist, _ songs: [Int]) {
        var q = p
        q.songs = songs
        db.putPlaylist(q)
        record("playlist.replace", playlist: p.id, ["playlist": .string(p.target), "songs": JSON(songs)])
        app.reloadPlaylists()
    }

    public func renamePlaylist(_ p: Playlist, _ name: String) {
        var q = p
        q.name = name
        db.putPlaylist(q)
        record("playlist.rename", playlist: p.id, ["playlist": .string(p.target), "name": .string(name)])
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
        db.addOp(OutboxOp(id: id, key: key ?? id + tag, json: JSON.object(op).text), replacing: key)
        app.sync.soon()
    }

    /// Ends the outbox key of an op on that playlist.
    nonisolated static func playlistTag(_ id: Int) -> String { "pl\(id)" }
}

extension AppModel {
    public var store: Store { Store(app: self) }

    /// A synced setting with the app's default for missing or null (plan 009).
    public func setting(_ name: String, _ def: Int) -> Int { settings[name]?.int ?? def }
    public func setting(_ name: String, _ def: Bool) -> Bool { settings[name]?.bool ?? def }
    public func setting(_ name: String, _ def: String) -> String { settings[name]?.string ?? def }
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
