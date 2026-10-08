import Foundation

/// Downloads (plan 012), as on the phone. The user pins things (an album, a
/// playlist, Favorites); the pins are turned into songs again whenever the
/// library or the lists change, and the files on disk follow: new songs are
/// fetched, songs no pin covers any more are deleted.
@MainActor @Observable
public final class Downloads {
    public enum State: Sendable {
        case idle, downloading, noNetwork, waitingForNetwork, full, noSpace, failed
    }

    public struct Status: Sendable {
        public var state: State = .idle
        /// Songs covered by pins, and how many of them are on disk.
        public var wanted = 0
        public var done = 0
        public var usedBytes = 0
        /// Full: how much more space the rest needs. Failed: the error.
        public var needBytes = 0
        public var error = ""
        public var current: Song?
        public var progress = 0.0
    }

    public private(set) var status = Status()

    @ObservationIgnored private unowned let app: AppModel
    @ObservationIgnored let dir: URL
    @ObservationIgnored private var running = false
    @ObservationIgnored private var again = false

    nonisolated public static let song = "song", album = "album", folder = "folder", artist = "artist",
        genre = "genre"
    nonisolated public static let playlist = "playlist"
    /// Favorites or Listen Later; `Pin.ref` is the `Store` mark kind.
    nonisolated public static let list = "list"
    nonisolated static let gb = 1 << 30
    /// Left free on the disk, so a full disk does not break the rest of the Mac.
    static let reserve = 200 << 20

    nonisolated public static func key(_ kind: String, _ ref: String) -> String { "\(kind):\(ref)" }

    init(app: AppModel) {
        self.app = app
        dir = app.files.appendingPathComponent("downloads")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    }

    public func pin(_ kind: String, _ ref: String, _ name: String) {
        app.db.putPin(Pin(key: Downloads.key(kind, ref), kind: kind, ref: ref, name: name, at: nowMs()))
        app.reloadPins()
        poke()
    }

    public func unpin(_ key: String) {
        app.db.deletePin(key)
        app.reloadPins()
        poke()
    }

    public func pinned(_ kind: String, _ ref: String) -> Bool {
        app.pins.contains { $0.key == Downloads.key(kind, ref) }
    }

    /// The local file for a song, if it is downloaded; asked by the player when it opens a song.
    public func file(_ song: Int) -> URL? {
        guard let d = app.downloaded[song], FileManager.default.fileExists(atPath: d.path) else { return nil }
        return URL(fileURLWithPath: d.path)
    }

    /// Looks at the pins and the files again: after a change, a new network, or the app coming back.
    public func poke() {
        if running {
            again = true
            return
        }
        Task { await run() }
    }

    /// On sign-out: the files belong to the account being left.
    func removeAll() {
        try? FileManager.default.removeItem(at: dir)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        status = Status()
    }

    /// The songs one download covers, for its button's "3/12".
    public func songs(_ kind: String, _ ref: String) -> [Song] {
        let key = Downloads.key(kind, ref)
        return covered(
            app.pins.filter { $0.key == key }, app.catalog, app.playlists, app.favorites, app.listenLater)
            ?? []
    }

    /// The songs the pins cover now.
    public var wanted: [Song]? {
        covered(app.pins, app.catalog, app.playlists, app.favorites, app.listenLater)
    }

    private func run() async {
        running = true
        defer {
            running = false
            if again {
                again = false
                poke()
            }
        }
        guard let songs = wanted else { return }
        let want = Dictionary(songs.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let fm = FileManager.default
        // Files nothing covers any more: removed from a playlist, unpinned, gone from the library.
        for d in app.downloaded.values {
            if want[d.song] == nil {
                try? fm.removeItem(atPath: d.path)
                app.db.deleteDownload(d.song)
            } else if !fm.fileExists(atPath: d.path) {
                // Deleted behind our back: fetch it again.
                app.db.deleteDownload(d.song)
            }
        }
        app.reloadDownloads()
        var have = app.downloaded
        let todo = songs.filter { s in have[s.id].map { s.size > 0 && $0.size != s.size } ?? true }
        var used = have.values.filter { want[$0.song] != nil }.reduce(0) { $0 + $1.size }
        // A part kept for resuming whose song is no longer wanted.
        let resumable = Set(todo.map(\.id))
        for f in (try? fm.contentsOfDirectory(atPath: dir.path)) ?? [] where f.hasSuffix(".part") {
            if let id = Int(f.prefix { $0 != "." }), resumable.contains(id) { continue }
            try? fm.removeItem(at: dir.appendingPathComponent(f))
        }
        update(.idle, songs.count, used, have)
        guard app.signedIn else { return }
        await app.lyrics.keep(songs.filter { have[$0.id] != nil })
        for (i, s) in todo.enumerated() {
            // Something changed while the last file was fetched: start over, so the newest pin goes first.
            if i > 0 && again { return }
            if !app.reachable { return update(.noNetwork, songs.count, used, have) }
            if app.sync.expensive && !app.prefs.downloadOnExpensive {
                return update(.waitingForNetwork, songs.count, used, have)
            }
            let limit = app.prefs.downloadLimitGb * Downloads.gb
            let replacing = have[s.id]?.size ?? 0
            if limit > 0 && used - replacing + s.size > limit {
                let rest = todo[i...].reduce(0) { $0 + $1.size }
                return update(.full, songs.count, used, have, need: used + rest - limit)
            }
            let free =
                (try? dir.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey]))?
                .volumeAvailableCapacityForImportantUsage.map(Int.init) ?? Int.max
            if free < s.size + Downloads.reserve {
                return update(.noSpace, songs.count, used, have, need: s.size + Downloads.reserve - free)
            }
            status.state = .downloading
            status.current = s
            status.progress = 0
            // A song in the song cache moves here rather than being fetched again.
            var file = app.cache.promote(s, to: dir)
            if file == nil {
                do {
                    file = try await fetch(s)
                } catch {
                    return fail(error)
                }
            }
            guard let file else { continue }
            if let old = have[s.id], old.path != file.path { try? fm.removeItem(atPath: old.path) }
            app.db.putDownload(Download(song: s.id, path: file.path, size: s.size, at: nowMs()))
            app.reloadDownloads()
            have = app.downloaded
            used += s.size - replacing
            status.done += 1
            status.usedBytes = used
        }
        await app.lyrics.keep(songs.filter { have[$0.id] != nil })
        update(.idle, songs.count, used, have)
    }

    /// Fetches one song to `<id>.<ext>`, through a `.part` file so a
    /// half-written file is never played; nil if the server no longer has
    /// it. A part left by a broken try is carried on with a range request.
    private func fetch(_ s: Song) async throws -> URL? {
        let ext = s.fileExtension.isEmpty ? s.format : s.fileExtension
        let file = dir.appendingPathComponent("\(s.id).\(ext)")
        let part = dir.appendingPathComponent("\(s.id).\(ext).part")
        let result = try await fetchFile(app.api, app.api.streamURL(s.id), to: part) { [weak self] p in
            Task { @MainActor in self?.status.progress = p }
        }
        if result == .gone {
            try? FileManager.default.removeItem(at: part)
            return nil
        }
        try? FileManager.default.removeItem(at: file)
        try FileManager.default.moveItem(at: part, to: file)
        return file
    }

    private func fail(_ e: Error) {
        status.state = .failed
        status.error = (e as? ApiError)?.message ?? e.localizedDescription
        status.current = nil
        // A dropped connection mid-file: try again shortly.
        Task {
            try? await Task.sleep(for: .seconds(30))
            poke()
        }
    }

    private func update(_ state: State, _ wanted: Int, _ used: Int, _ have: [Int: Download], need: Int = 0) {
        let done = self.wanted?.filter { have[$0.id] != nil }.count ?? 0
        status = Status(state: state, wanted: wanted, done: done, usedBytes: used, needBytes: need)
    }
}

/// The songs `pins` cover, newest pin first so what was just clicked starts
/// first; nil while the catalogue is empty, because an empty catalogue would
/// read as "delete every download".
public func covered(_ pins: [Pin], _ c: Catalog, _ playlists: [Playlist], _ favs: [Mark], _ later: [Mark])
    -> [Song]?
{
    guard !c.songs.isEmpty else { return nil }
    var ids: [Int] = []
    var seen = Set<Int>()
    func add(_ id: Int) { if seen.insert(id).inserted { ids.append(id) } }
    for p in pins.sorted(by: { $0.at > $1.at }) {
        switch p.kind {
        case Downloads.song: Int(p.ref).map(add)
        case Downloads.album: c.albums.first { $0.id == p.ref }?.songs.forEach { add($0.id) }
        case Downloads.folder: c.folders[p.ref]?.allSongs().forEach { add($0.id) }
        case Downloads.artist:
            c.artists.first { $0.name.lowercased() == p.ref.lowercased() }?.songs.forEach { add($0.id) }
        case Downloads.genre:
            c.genres.first { $0.name.lowercased() == p.ref.lowercased() }?.songs.forEach { add($0.id) }
        case Downloads.playlist: playlists.first { String($0.id) == p.ref }?.songs.forEach(add)
        case Downloads.list: (p.ref == Store.fav ? favs : later).forEach { add($0.song) }
        default: break
        }
    }
    return c.songsOf(ids)
}

/// "1.2 GB", "340 MB"
public func formatBytes(_ n: Int) -> String {
    if n >= 1 << 30 { return String(format: "%.1f GB", Double(n) / Double(1 << 30)) }
    if n >= 1 << 20 { return "\(n >> 20) MB" }
    return "\(n >> 10) KB"
}

enum FetchResult { case done, gone }

/// Appends a file from our server to `part`, from where it ends (a Range
/// request), reporting progress; `gone` when the server no longer has it.
func fetchFile(_ api: Api, _ url: URL, to part: URL, progress: @escaping @Sendable (Double) -> Void)
    async throws
    -> FetchResult
{
    let fm = FileManager.default
    if !fm.fileExists(atPath: part.path) { fm.createFile(atPath: part.path, contents: nil) }
    var have = ((try? fm.attributesOfItem(atPath: part.path))?[.size] as? NSNumber)?.intValue ?? 0
    var req = api.request(url)
    req.timeoutInterval = 20
    if have > 0 { req.setValue("bytes=\(have)-", forHTTPHeaderField: "Range") }
    let (stream, resp) = try await api.session.bytes(for: req)
    let status = (resp as? HTTPURLResponse)?.statusCode ?? 0
    switch status {
    case 404, 410: return .gone
    // The part is longer than the file: the file changed. Start again.
    case 416:
        try? fm.removeItem(at: part)
        throw ApiError(code: 416, message: "The file changed on the server")
    case 200: have = 0  // the server ignored the range
    case 206: break
    default: throw ApiError(code: status, message: "HTTP \(status)")
    }
    let total = resp.expectedContentLength >= 0 ? have + Int(resp.expectedContentLength) : 0
    let out = try FileHandle(forWritingTo: part)
    defer { try? out.close() }
    try out.truncate(atOffset: UInt64(have))
    try out.seek(toOffset: UInt64(have))
    var buf = Data()
    buf.reserveCapacity(256 << 10)
    var shown = have
    for try await chunk in stream.chunks(256 << 10) {
        buf.append(contentsOf: chunk)
        try out.write(contentsOf: buf)
        have += buf.count
        buf.removeAll(keepingCapacity: true)
        if total > 0 && have - shown > 512 << 10 {
            shown = have
            progress(Double(have) / Double(total))
        }
    }
    // Against what the server sent: the song list may be older than the file.
    if total > 0 && have != total {
        try? fm.removeItem(at: part)
        throw ApiError(code: 0, message: "Got \(have) of \(total) bytes")
    }
    return .done
}

extension URLSession.AsyncBytes {
    /// The bytes in arrays of up to `n`: one at a time is slow.
    func chunks(_ n: Int) -> AsyncThrowingStream<[UInt8], Error> {
        AsyncThrowingStream { cont in
            let task = Task {
                var chunk: [UInt8] = []
                chunk.reserveCapacity(n)
                do {
                    for try await b in self {
                        chunk.append(b)
                        if chunk.count == n {
                            cont.yield(chunk)
                            chunk.removeAll(keepingCapacity: true)
                        }
                    }
                    if !chunk.isEmpty { cont.yield(chunk) }
                    cont.finish()
                } catch { cont.finish(throwing: error) }
            }
            cont.onTermination = { _ in task.cancel() }
        }
    }
}
