import Foundation

/// The working copy (plan 006): the catalogue, the user's synced data, and
/// the outbox of changes not yet on the server. The same tables as the
/// phone's Room database (plan 024). Lists of song IDs are kept as
/// comma-separated text: they are only ever read and written whole.
public final class DB: @unchecked Sendable {
    public let sql: SQLite

    public init(path: String) throws {
        sql = try SQLite(path: path)
        try migrate()
    }

    /// Each step runs once, in order; a new one goes at the end. The outbox
    /// is never dropped: offline edits are never lost (AGENTS.md).
    private func migrate() throws {
        let steps = [
            """
            CREATE TABLE song (id INTEGER PRIMARY KEY, path TEXT NOT NULL, title TEXT NOT NULL,
              artist TEXT NOT NULL, artists TEXT NOT NULL, album TEXT NOT NULL, albumArtist TEXT NOT NULL,
              composer TEXT NOT NULL, genres TEXT NOT NULL, year INTEGER NOT NULL, track INTEGER NOT NULL,
              disc INTEGER NOT NULL, durationMs INTEGER NOT NULL, format TEXT NOT NULL, bitrate INTEGER NOT NULL,
              sampleRate INTEGER NOT NULL, bitDepth INTEGER NOT NULL, size INTEGER NOT NULL,
              hasArt INTEGER NOT NULL, hasLyrics INTEGER NOT NULL, addedAt INTEGER NOT NULL,
              missing INTEGER NOT NULL, art TEXT NOT NULL);
            CREATE TABLE playlist (id INTEGER PRIMARY KEY, name TEXT NOT NULL, path TEXT NOT NULL,
              shared INTEGER NOT NULL, songs TEXT NOT NULL, ref TEXT NOT NULL);
            CREATE TABLE queue (id TEXT PRIMARY KEY, name TEXT NOT NULL, songs TEXT NOT NULL,
              currentSong INTEGER NOT NULL, positionMs INTEGER NOT NULL, shuffle INTEGER NOT NULL,
              repeat TEXT NOT NULL, usedAt INTEGER NOT NULL);
            CREATE TABLE mark (kind TEXT NOT NULL, song INTEGER NOT NULL, at INTEGER NOT NULL,
              deleted INTEGER NOT NULL, PRIMARY KEY (kind, song));
            CREATE TABLE resume (song INTEGER PRIMARY KEY, positionMs INTEGER NOT NULL, at INTEGER NOT NULL,
              deleted INTEGER NOT NULL);
            CREATE TABLE setting (name TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE outbox (seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL, key TEXT NOT NULL,
              json TEXT NOT NULL);
            CREATE TABLE play_stat (song INTEGER PRIMARY KEY, count INTEGER NOT NULL,
              lastPlayedAt INTEGER NOT NULL);
            CREATE TABLE pin (key TEXT PRIMARY KEY, kind TEXT NOT NULL, ref TEXT NOT NULL, name TEXT NOT NULL,
              at INTEGER NOT NULL);
            CREATE TABLE download (song INTEGER PRIMARY KEY, path TEXT NOT NULL, size INTEGER NOT NULL,
              at INTEGER NOT NULL);
            CREATE TABLE lyrics (song INTEGER PRIMARY KEY, text TEXT NOT NULL, at INTEGER NOT NULL);
            CREATE TABLE thumb (key TEXT PRIMARY KEY, data BLOB NOT NULL);
            """
        ]
        let have = sql.userVersion
        for (i, step) in steps.enumerated() where i >= have {
            try sql.transaction { try sql.exec(step) }
            sql.userVersion = i + 1
        }
    }

    /// Everything but the outbox can be fetched again; used when another user signs in.
    public func wipe() throws {
        try sql.exec(
            """
            DELETE FROM song; DELETE FROM playlist; DELETE FROM queue; DELETE FROM mark; DELETE FROM resume;
            DELETE FROM setting; DELETE FROM outbox; DELETE FROM play_stat; DELETE FROM pin; DELETE FROM download;
            DELETE FROM lyrics;
            """)
    }

    // MARK: Songs

    public func songs() -> [Song] {
        (try? sql.query("SELECT * FROM song WHERE missing = 0") { r in
            Song(
                id: r.int(0), path: r.text(1), title: r.text(2), artist: r.text(3), artists: split(r.text(4)),
                album: r.text(5), albumArtist: r.text(6), composer: r.text(7), genres: split(r.text(8)),
                year: r.int(9), track: r.int(10), disc: r.int(11), durationMs: r.int(12), format: r.text(13),
                bitrate: r.int(14), sampleRate: r.int(15), bitDepth: r.int(16), size: r.int(17),
                hasArt: r.bool(18), hasLyrics: r.bool(19), addedAt: r.int(20), missing: r.bool(21),
                art: r.text(22))
        }) ?? []
    }

    public func putSongs(_ songs: [Song]) throws {
        try sql.runMany(
            "INSERT OR REPLACE INTO song VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            songs.map { s in
                ([
                    s.id, s.path, s.title, s.artist, s.artists.joined(separator: sep), s.album, s.albumArtist,
                    s.composer, s.genres.joined(separator: sep), s.year, s.track, s.disc, s.durationMs,
                    s.format,
                    s.bitrate, s.sampleRate, s.bitDepth, s.size, s.hasArt, s.hasLyrics, s.addedAt, s.missing,
                    s.art,
                ] as [any SQLBindable]).sql
            })
    }

    // MARK: Playlists

    public func playlists() -> [Playlist] {
        (try? sql.query(
            "SELECT id, name, path, shared, songs, ref FROM playlist ORDER BY shared, name COLLATE NOCASE"
        ) { r in
            Playlist(
                id: r.int(0), name: r.text(1), path: r.text(2), shared: r.bool(3), songs: ids(r.text(4)),
                ref: r.text(5))
        }) ?? []
    }

    public func putPlaylist(_ p: Playlist) {
        try? sql.run(
            "INSERT OR REPLACE INTO playlist VALUES (?,?,?,?,?,?)",
            ([p.id, p.name, p.path, p.shared, join(p.songs), p.ref] as [any SQLBindable]).sql)
    }

    public func deletePlaylist(_ id: Int) { try? sql.run("DELETE FROM playlist WHERE id = ?", [.int(id)]) }

    public func localPlaylists() -> [Playlist] { playlists().filter { $0.id < 0 } }

    // MARK: Queues

    public func queues() -> [QueueRow] {
        (try? sql.query("SELECT * FROM queue ORDER BY usedAt DESC") { r in
            QueueRow(
                id: r.text(0), name: r.text(1), songs: ids(r.text(2)), currentSong: r.int(3),
                positionMs: r.int(4),
                shuffle: r.bool(5), repeatMode: r.text(6), usedAt: r.int(7))
        }) ?? []
    }

    public func putQueue(_ q: QueueRow) {
        try? sql.run(
            "INSERT OR REPLACE INTO queue VALUES (?,?,?,?,?,?,?,?)",
            ([q.id, q.name, join(q.songs), q.currentSong, q.positionMs, q.shuffle, q.repeatMode, q.usedAt]
                as [any SQLBindable]).sql)
    }

    public func deleteQueue(_ id: String) { try? sql.run("DELETE FROM queue WHERE id = ?", [.text(id)]) }

    // MARK: Favorites, Listen Later, resume points, settings

    public func marks(_ kind: String) -> [Mark] {
        (try? sql.query(
            "SELECT kind, song, at, deleted FROM mark WHERE kind = ? AND deleted = 0 ORDER BY at DESC",
            [.text(kind)]
        ) { r in Mark(kind: r.text(0), song: r.int(1), at: r.int(2), deleted: r.bool(3)) }) ?? []
    }

    public func mark(_ kind: String, _ song: Int) -> Mark? {
        try? sql.query(
            "SELECT kind, song, at, deleted FROM mark WHERE kind = ? AND song = ?", [.text(kind), .int(song)]
        ) { r in Mark(kind: r.text(0), song: r.int(1), at: r.int(2), deleted: r.bool(3)) }.first
    }

    public func putMark(_ m: Mark) {
        try? sql.run(
            "INSERT OR REPLACE INTO mark VALUES (?,?,?,?)",
            ([m.kind, m.song, m.at, m.deleted] as [any SQLBindable]).sql)
    }

    public func resumes() -> [Resume] {
        (try? sql.query("SELECT song, positionMs, at, deleted FROM resume WHERE deleted = 0 ORDER BY at DESC")
        { r in
            Resume(song: r.int(0), positionMs: r.int(1), at: r.int(2), deleted: r.bool(3))
        }) ?? []
    }

    public func resume(_ song: Int) -> Resume? {
        try? sql.query("SELECT song, positionMs, at, deleted FROM resume WHERE song = ?", [.int(song)]) { r in
            Resume(song: r.int(0), positionMs: r.int(1), at: r.int(2), deleted: r.bool(3))
        }.first
    }

    public func putResume(_ r: Resume) {
        try? sql.run(
            "INSERT OR REPLACE INTO resume VALUES (?,?,?,?)",
            ([r.song, r.positionMs, r.at, r.deleted] as [any SQLBindable]).sql)
    }

    public func settings() -> [String: JSON] {
        let rows =
            (try? sql.query("SELECT name, value FROM setting") { ($0.text(0), JSON.parse($0.text(1))) }) ?? []
        return Dictionary(rows, uniquingKeysWith: { a, _ in a })
    }

    public func putSetting(_ name: String, _ value: JSON) {
        if value == .null {
            try? sql.run("DELETE FROM setting WHERE name = ?", [.text(name)])
        } else {
            try? sql.run("INSERT OR REPLACE INTO setting VALUES (?,?)", [.text(name), .text(value.text)])
        }
    }

    // MARK: Outbox

    public func outbox(limit: Int = Int.max) -> [OutboxOp] {
        (try? sql.query("SELECT id, key, json FROM outbox ORDER BY seq LIMIT ?", [.int(limit)]) { r in
            OutboxOp(id: r.text(0), key: r.text(1), json: r.text(2))
        }) ?? []
    }

    public func outboxSize() -> Int {
        (try? sql.query("SELECT count(*) FROM outbox") { $0.int(0) }.first) ?? 0
    }

    /// Drops earlier ops with the same key (only the latest matters), then adds this one.
    public func addOp(_ op: OutboxOp, replacing key: String?) {
        try? sql.transaction {
            if let key { try sql.run("DELETE FROM outbox WHERE key = ?", [.text(key)]) }
            try sql.run(
                "INSERT INTO outbox (id, key, json) VALUES (?,?,?)",
                [.text(op.id), .text(op.key), .text(op.json)])
        }
    }

    public func ackOps(_ ids: [String]) {
        try? sql.runMany("DELETE FROM outbox WHERE id = ?", ids.map { [.text($0)] })
    }

    // MARK: Play counts

    public func playStats() -> [Int: PlayStat] {
        let rows =
            (try? sql.query("SELECT song, count, lastPlayedAt FROM play_stat") {
                PlayStat(song: $0.int(0), count: $0.int(1), lastPlayedAt: $0.int(2))
            }) ?? []
        return Dictionary(rows.map { ($0.song, $0) }, uniquingKeysWith: { a, _ in a })
    }

    public func replacePlayStats(_ stats: [PlayStat]) throws {
        try sql.transaction {
            try sql.exec("DELETE FROM play_stat")
            try sql.runMany(
                "INSERT INTO play_stat VALUES (?,?,?)",
                stats.map { [.int($0.song), .int($0.count), .int($0.lastPlayedAt)] })
        }
    }

    // MARK: Downloads (plan 012)

    public func pins() -> [Pin] {
        (try? sql.query("SELECT key, kind, ref, name, at FROM pin ORDER BY at DESC") { r in
            Pin(key: r.text(0), kind: r.text(1), ref: r.text(2), name: r.text(3), at: r.int(4))
        }) ?? []
    }

    public func putPin(_ p: Pin) {
        try? sql.run(
            "INSERT OR REPLACE INTO pin VALUES (?,?,?,?,?)",
            ([p.key, p.kind, p.ref, p.name, p.at] as [any SQLBindable]).sql)
    }

    public func deletePin(_ key: String) { try? sql.run("DELETE FROM pin WHERE key = ?", [.text(key)]) }

    public func movePin(from old: String, to key: String, ref: String) {
        try? sql.run("UPDATE pin SET key = ?, ref = ? WHERE key = ?", [.text(key), .text(ref), .text(old)])
    }

    public func downloads() -> [Int: Download] {
        let rows =
            (try? sql.query("SELECT song, path, size, at FROM download") {
                Download(song: $0.int(0), path: $0.text(1), size: $0.int(2), at: $0.int(3))
            }) ?? []
        return Dictionary(rows.map { ($0.song, $0) }, uniquingKeysWith: { a, _ in a })
    }

    public func putDownload(_ d: Download) {
        try? sql.run(
            "INSERT OR REPLACE INTO download VALUES (?,?,?,?)",
            ([d.song, d.path, d.size, d.at] as [any SQLBindable]).sql)
    }

    public func deleteDownload(_ song: Int) {
        try? sql.run("DELETE FROM download WHERE song = ?", [.int(song)])
    }

    // MARK: Lyrics and thumbnails

    public func lyrics(_ song: Int) -> String? {
        try? sql.query("SELECT text FROM lyrics WHERE song = ?", [.int(song)]) { $0.text(0) }.first
    }

    public func lyricsSongs() -> Set<Int> {
        Set((try? sql.query("SELECT song FROM lyrics") { $0.int(0) }) ?? [])
    }

    /// Every song's kept lyrics, for searching them offline.
    public func allLyrics() -> [(song: Int, text: String)] {
        (try? sql.query("SELECT song, text FROM lyrics") { ($0.int(0), $0.text(1)) }) ?? []
    }

    public func putLyrics(_ song: Int, _ text: String) {
        try? sql.run(
            "INSERT OR REPLACE INTO lyrics VALUES (?,?,?)", [.int(song), .text(text), .int(nowMs())])
    }

    public func deleteLyrics(_ song: Int) { try? sql.run("DELETE FROM lyrics WHERE song = ?", [.int(song)]) }

    public func thumb(_ key: String) -> Data? {
        try? sql.query("SELECT data FROM thumb WHERE key = ?", [.text(key)]) { $0.blob(0) }.first
    }

    public func artKeys() -> [String] {
        (try? sql.query("SELECT DISTINCT art FROM song WHERE missing = 0 AND hasArt = 1 AND art != ''") {
            $0.text(0)
        }) ?? []
    }

    public func thumbKeys() -> Set<String> {
        Set((try? sql.query("SELECT key FROM thumb") { $0.text(0) }) ?? [])
    }

    public func putThumbs(_ thumbs: [(String, Data)]) {
        try? sql.runMany(
            "INSERT OR REPLACE INTO thumb VALUES (?,?)", thumbs.map { [.text($0.0), .blob($0.1)] })
    }
}

/// Joins and splits the text form of a list (artists, genres).
let sep = "\u{1f}"
private func split(_ s: String) -> [String] { s.isEmpty ? [] : s.components(separatedBy: sep) }
func ids(_ s: String) -> [Int] { s.isEmpty ? [] : s.split(separator: ",").compactMap { Int($0) } }
func join(_ ids: [Int]) -> String { ids.map(String.init).joined(separator: ",") }
