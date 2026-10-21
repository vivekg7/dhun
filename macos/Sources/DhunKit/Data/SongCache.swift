import Foundation

/// The song cache (plan 019), as on the phone: original files, fetched whole
/// at the network's full speed, so a song is here seconds after it starts and
/// a later gap in the network does not matter. It keeps the song playing
/// (every song played is kept) and, while playing, the next ten of the queue,
/// or two on an expensive network. The player reads a file while it is still
/// arriving (`open`).
///
/// A long file (an episode, a book, a mix: `Playback.isLong`) is not fetched
/// whole but as a window around where it plays (plan 037): the minutes ahead
/// that `longFiles.cacheAhead` says, ten behind, in a `.window` file the
/// fetcher fills wherever the reader is. A window is never a cached song: it
/// sits outside the music budget, is dropped as soon as its file leaves the
/// plan, and is never promoted to a download.
///
/// Plain files beside Downloads, so a cached song becomes a download by a
/// rename; a file's modification time is when it was last played, for
/// dropping the oldest at the limit. Never holds a downloaded song. A part
/// or window left by an earlier run is dropped: which of its bytes arrived
/// was only ever known in memory.
@MainActor @Observable
public final class SongCache {
    /// Bytes in the cache, for Settings.
    public private(set) var used = 0
    /// Songs whole in the cache: offline, they play, so the lists do not dim them.
    public private(set) var songs: Set<Int> = []

    @ObservationIgnored private unowned let app: AppModel
    @ObservationIgnored let dir: URL
    @ObservationIgnored private var entries: [Int: Fetch] = [:]
    @ObservationIgnored private var planned = false

    static let aheadExpensive = 2
    static let ahead = 10
    /// How much of a long file is kept behind the reader (plan 037).
    static let behindMinutes = 10
    static let partial = [".part", ".window", ".tmp"]

    init(app: AppModel) {
        self.app = app
        dir = app.files.appendingPathComponent("cache")
        let fm = FileManager.default
        try? fm.createDirectory(at: dir, withIntermediateDirectories: true)
        for f in (try? fm.contentsOfDirectory(atPath: dir.path)) ?? [] where !Self.isWhole(f) {
            try? fm.removeItem(at: dir.appendingPathComponent(f))
        }
        measure()
    }

    /// Where a song's bytes come from for the player: a download, then a whole
    /// cached file, then one the cache fills while it plays (the stream).
    public func open(_ s: Song) -> Bytes? {
        if let f = app.downloads.file(s.id) { return try? FileBytes(f) }
        if let f = complete(s.id) { return try? FileBytes(f) }
        if let e = entries[s.id] { return e.file }
        let e = start(s)
        poke()
        return e?.file
    }

    /// The song is still arriving and the last try to fetch it failed: the player is waiting for the network.
    public func failing(_ song: Int) -> Bool { entries[song]?.file.failing ?? false }

    /// A cached song's whole file, marked as just played; nil if it is not all here.
    func complete(_ song: Int) -> URL? {
        guard let s = app.catalog.byId[song] else { return nil }
        let f = dir.appendingPathComponent(name(s))
        guard FileManager.default.fileExists(atPath: f.path) else { return nil }
        try? FileManager.default.setAttributes([.modificationDate: Date()], ofItemAtPath: f.path)
        return f
    }

    /// Moves a cached song into `dir`, as a download (REQUIREMENTS: never fetched twice); nil if it is not all here.
    func promote(_ s: Song, to dir: URL) -> URL? {
        guard let from = complete(s.id) else { return nil }
        let size = ((try? FileManager.default.attributesOfItem(atPath: from.path))?[.size] as? NSNumber)?
            .intValue
        if s.size > 0 && size != s.size { return nil }
        let to = dir.appendingPathComponent(from.lastPathComponent)
        try? FileManager.default.removeItem(at: to)
        guard (try? FileManager.default.moveItem(at: from, to: to)) != nil else { return nil }
        measure()
        return to
    }

    func removeAll() {
        for e in entries.values { e.cancel() }
        entries = [:]
        try? FileManager.default.removeItem(at: dir)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        measure()
    }

    /// Looks again at what to fetch: the song or the queue changed, play
    /// started, the network changed, the limit changed.
    public func poke() {
        guard !planned else { return }
        planned = true
        Task {
            planned = false
            run()
        }
    }

    private func run() {
        let limit = app.prefs.cacheLimitGb * Downloads.gb
        let plan = app.playback.upcoming(
            app.playback.isPlaying ? (app.sync.expensive ? Self.aheadExpensive : Self.ahead) : 0
        )
        .filter { app.downloads.file($0.id) == nil }
        let wanted = Set(plan.map(\.id))
        // A fetch nothing wants any more (a skip, another queue) stops; the
        // playing song is first in the plan, so it never does.
        for (id, e) in entries where !wanted.contains(id) {
            e.cancel()
            entries[id] = nil
            try? FileManager.default.removeItem(at: e.file.url)
        }
        trim(limit, keep: wanted)
        // One at a time, in the plan's order, as the phone fetches. A window
        // paused at its end does not count: the next file is fetched meanwhile.
        if !entries.values.contains(where: \.active),
            let next = plan.first(where: { complete($0.id) == nil && entries[$0.id] == nil }),
            limit > 0 || next.id == plan.first?.id
        {
            // The next file's window starts at its start; the one playing anchors on its reader.
            if let e = start(next), next.id != plan.first?.id { e.anchor(0) }
        }
    }

    @discardableResult
    private func start(_ s: Song) -> Fetch? {
        guard s.size > 0 else { return nil }
        let (ahead, behind) = window(for: s)
        let part = dir.appendingPathComponent(name(s) + (ahead > 0 ? ".window" : ".part"))
        try? FileManager.default.removeItem(at: part)
        guard let file = try? SparseFile(part, size: Int64(s.size)) else { return nil }
        let e = Fetch(
            request: app.api.request(app.api.streamURL(s.id)), file: file, ahead: ahead, behind: behind)
        let api = app.api
        e.reachable = { ok in api.reachable?(ok) }
        e.completed = { [weak self] in Task { @MainActor in self?.finished(s, e) } }
        e.gone = { [weak self] in Task { @MainActor in self?.dropped(s.id, e) } }
        e.paused = { [weak self] in Task { @MainActor in self?.poke() } }
        entries[s.id] = e
        e.start()
        return e
    }

    /// The bytes fetched ahead of the reader and kept behind it: a window for
    /// a long file whose size and length are known, else (0, 0), fetched whole.
    private func window(for s: Song) -> (ahead: Int64, behind: Int64) {
        let perMinute = s.size > 0 && s.durationMs > 0 ? Int64(s.size) * 60_000 / Int64(s.durationMs) : 0
        guard perMinute > 0, app.playback.isLong(s) else { return (0, 0) }
        let minutes = Int64(app.setting(Synced.cacheAhead))
        return minutes > 0 ? (minutes * perMinute, Int64(Self.behindMinutes) * perMinute) : (0, 0)
    }

    private func finished(_ s: Song, _ e: Fetch) {
        guard entries[s.id] === e else { return }
        entries[s.id] = nil
        // The reader keeps its descriptor across the rename.
        try? FileManager.default.moveItem(at: e.file.url, to: dir.appendingPathComponent(name(s)))
        // With the cache off, a song is fetched only to play it.
        if app.prefs.cacheLimitGb == 0 {
            try? FileManager.default.removeItem(at: dir.appendingPathComponent(name(s)))
        }
        measure()
        poke()
    }

    private func dropped(_ id: Int, _ e: Fetch) {
        guard entries[id] === e else { return }
        entries[id] = nil
        try? FileManager.default.removeItem(at: e.file.url)
        poke()
    }

    /// Drops the songs played longest ago until the cache is within `limit`;
    /// never one in the plan. Also drops any song since downloaded, and, once
    /// it has left the plan, a long file an earlier version or setting kept
    /// whole, which is cached as a window now. Parts and windows are not
    /// songs: `run` drops them with their fetch.
    private func trim(_ limit: Int, keep: Set<Int>) {
        let fm = FileManager.default
        guard
            let files = try? fm.contentsOfDirectory(
                at: dir, includingPropertiesForKeys: [.contentModificationDateKey, .fileSizeKey])
        else { return }
        var rest: [(URL, Date, Int)] = []
        for f in files where Self.isWhole(f.lastPathComponent) {
            guard let id = Int(f.lastPathComponent.prefix { $0 != "." }) else { continue }
            let windowed = app.catalog.byId[id].map { window(for: $0).ahead > 0 } ?? false
            if app.downloads.file(id) != nil || (windowed && !keep.contains(id)) {
                try? fm.removeItem(at: f)
                continue
            }
            let v = try? f.resourceValues(forKeys: [.contentModificationDateKey, .fileSizeKey])
            rest.append(
                (f, v?.contentModificationDate ?? .distantPast, keep.contains(id) ? -1 : v?.fileSize ?? 0))
        }
        var total = rest.reduce(0) { $0 + max(0, $1.2) }
        for (f, _, size) in rest.sorted(by: { $0.1 < $1.1 }) where total > limit && size >= 0 {
            try? fm.removeItem(at: f)
            total -= size
        }
        measure()
    }

    private func measure() {
        let files =
            (try? FileManager.default.contentsOfDirectory(
                at: dir, includingPropertiesForKeys: [.totalFileAllocatedSizeKey])) ?? []
        // Blocks, not length: a window is a sparse file as long as the whole book.
        used = files.reduce(0) {
            $0 + ((try? $1.resourceValues(forKeys: [.totalFileAllocatedSizeKey]).totalFileAllocatedSize) ?? 0)
        }
        songs = Set(
            files.filter { Self.isWhole($0.lastPathComponent) }.compactMap {
                Int($0.lastPathComponent.prefix { $0 != "." })
            })
    }

    /// A file by its final name, not a part, a window or a roll in progress.
    private static func isWhole(_ name: String) -> Bool { !partial.contains { name.hasSuffix($0) } }

    private func name(_ s: Song) -> String {
        "\(s.id).\(s.fileExtension.isEmpty ? s.format : s.fileExtension)"
    }
}
