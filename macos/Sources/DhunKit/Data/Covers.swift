import AppKit
import Foundation

/// Cover art (plan 019), as on the phone: kept as files named by the
/// server's art key, so an album's cover is fetched and stored once and a
/// kept one never needs asking about again. Every cover has a 128 px
/// thumbnail in the database, fetched after a sync, so list rows never wait;
/// larger covers are files fetched when first shown, and the `keep` used
/// most recently stay, those of downloaded songs always. The server already
/// resizes, so no image library.
public final class Covers: @unchecked Sendable {
    public static let thumb = 128
    /// The phone's figure: enough full covers for everything played in a long while.
    static let keep = 500
    static let sizes = [128, 256, 512, 1024]

    private weak var app: AppModel?
    private let db: DB
    private let api: Api
    private let dir: URL
    private let memory = NSCache<NSString, NSImage>()
    private let lock = NSLock()
    private var fetches: [String: Task<URL?, Never>] = [:]
    private var thumbsRunning = false
    private var written = 0

    @MainActor init(app: AppModel) {
        self.app = app
        db = app.db
        api = app.api
        dir = app.files.appendingPathComponent("covers")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        memory.totalCostLimit = 64 << 20
    }

    /// A server without art keys (before 0.1.4): one cover per song.
    private func key(_ s: Song) -> String { s.art.isEmpty ? "s\(s.id)" : s.art }

    /// The thumbnail, from the database; nil until it has been fetched. Cheap enough for a list row.
    public func thumbnail(_ s: Song) -> NSImage? {
        guard s.hasArt, !s.art.isEmpty else { return nil }
        let mem = "\(s.art)/t" as NSString
        if let i = memory.object(forKey: mem) { return i }
        guard let data = db.thumb(s.art), !data.isEmpty, let img = NSImage(data: data) else { return nil }
        memory.setObject(img, forKey: mem, cost: data.count * 4)
        return img
    }

    /// The cover `image` would give, if it is in memory already: a view
    /// shows it in its first frame rather than the thumbnail, then the cover.
    public func cached(_ s: Song, px: Int) -> NSImage? {
        guard s.hasArt else { return nil }
        if px <= Covers.thumb { return thumbnail(s) }
        let size = Covers.sizes.first { $0 >= px } ?? Covers.sizes.last!
        return memory.object(forKey: "\(key(s))/\(size)" as NSString)
    }

    /// The cover at least `px` across where the server has it that large;
    /// offline, a smaller kept copy beats none.
    public func image(_ s: Song, px: Int) async -> NSImage? {
        guard s.hasArt else { return nil }
        if px <= Covers.thumb, let t = thumbnail(s) { return t }
        let size = Covers.sizes.first { $0 >= px } ?? Covers.sizes.last!
        let key = key(s)
        let mem = "\(key)/\(size)" as NSString
        if let i = memory.object(forKey: mem) { return i }
        var file = kept(key, size)
        if file == nil { file = await fetch(s.id, key, size) }
        if file == nil { file = kept(key, 0) }
        guard let file, let img = NSImage(contentsOf: file) else { return thumbnail(s) }
        memory.setObject(img, forKey: mem, cost: size * size * 4)
        return img
    }

    /// Fetches every cover's thumbnail not yet here, after each sync: all of
    /// them the first time, then only new covers. A failure stops the run;
    /// the next sync carries on. Thumbnails no song shows any more are dropped.
    public func fetchThumbs() async {
        guard
            lock.withLock({ () -> Bool in
                if thumbsRunning { return false }
                thumbsRunning = true
                return true
            })
        else { return }
        defer { lock.withLock { thumbsRunning = false } }
        let keys = Set(db.artKeys())
        guard !keys.isEmpty else { return }
        let have = db.thumbKeys()
        let gone = Array(have.subtracting(keys))
        if !gone.isEmpty { try? db.sql.runMany("DELETE FROM thumb WHERE key = ?", gone.map { [.text($0)] }) }
        let missing = Array(keys.subtracting(have))
        var got = 0
        for start in stride(from: 0, to: missing.count, by: 50) {
            guard let batch = try? await api.thumbs(Array(missing[start..<min(start + 50, missing.count)]))
            else { return }
            db.putThumbs(batch.map { ($0.key, Data(base64Encoded: $0.value) ?? Data()) })
            got += batch.count
        }
        // New thumbnails: lists showing placeholders draw again.
        if got > 0 { await MainActor.run { self.app?.thumbsArrived += 1 } }
    }

    /// The kept file for `key` of at least `size` px. One per key: a larger fetch replaces it.
    private func kept(_ key: String, _ size: Int) -> URL? {
        for s in Covers.sizes where s >= size {
            let f = dir.appendingPathComponent("\(key).\(s)")
            if FileManager.default.fileExists(atPath: f.path) {
                try? FileManager.default.setAttributes([.modificationDate: Date()], ofItemAtPath: f.path)
                return f
            }
        }
        return nil
    }

    /// One request per cover, however many views want it.
    private func fetch(_ song: Int, _ key: String, _ size: Int) async -> URL? {
        let name = "\(key).\(size)"
        let task: Task<URL?, Never> = lock.withLock {
            if let t = fetches[name] { return t }
            let t = Task { await self.download(song, key, size) }
            fetches[name] = t
            return t
        }
        let r = await task.value
        lock.withLock { fetches[name] = nil }
        return r
    }

    private func download(_ song: Int, _ key: String, _ size: Int) async -> URL? {
        guard let data = await api.data(api.artURL(song, size: size)) else { return nil }
        let file = dir.appendingPathComponent("\(key).\(size)")
        guard (try? data.write(to: file, options: .atomic)) != nil else { return nil }
        for s in Covers.sizes where s < size {
            try? FileManager.default.removeItem(at: dir.appendingPathComponent("\(key).\(s)"))
        }
        if lock.withLock({ () -> Int in
            written += 1
            return written
        }) % 25 == 0 {
            await evict()
        }
        return file
    }

    /// Drops the covers used longest ago beyond `keep`, never a downloaded song's.
    private func evict() async {
        let fm = FileManager.default
        guard
            let files = try? fm.contentsOfDirectory(
                at: dir, includingPropertiesForKeys: [.contentModificationDateKey]),
            files.count > Covers.keep
        else { return }
        let downloaded: Set<String> = await MainActor.run {
            guard let app = self.app else { return [] }
            return Set(app.downloaded.keys.compactMap { app.catalog.byId[$0] }.map { self.key($0) })
        }
        let dated = files.filter { !downloaded.contains($0.deletingPathExtension().lastPathComponent) }.map {
            (
                $0,
                (try? $0.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate)
                    ?? .distantPast
            )
        }
        for (f, _) in dated.sorted(by: { $0.1 < $1.1 }).prefix(files.count - Covers.keep) {
            try? fm.removeItem(at: f)
        }
    }

    public func removeAll() {
        try? FileManager.default.removeItem(at: dir)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        memory.removeAllObjects()
    }
}
