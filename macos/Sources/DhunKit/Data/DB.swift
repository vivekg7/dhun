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

    /// Each step runs once, in order; a new one goes at the end. No step
    /// touches the outbox: an upgrade never loses an offline edit (AGENTS.md).
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
            """,
            // Podcasts and audiobooks (plan 031). Chapters are the server's JSON, "" for none.
            """
            ALTER TABLE song ADD COLUMN kind TEXT NOT NULL DEFAULT '';
            ALTER TABLE song ADD COLUMN date TEXT NOT NULL DEFAULT '';
            ALTER TABLE song ADD COLUMN notes TEXT NOT NULL DEFAULT '';
            ALTER TABLE song ADD COLUMN chapters TEXT NOT NULL DEFAULT '';
            ALTER TABLE song ADD COLUMN groupKey TEXT NOT NULL DEFAULT '';
            ALTER TABLE song ADD COLUMN groupTitle TEXT NOT NULL DEFAULT '';
            ALTER TABLE song ADD COLUMN groupIndex INTEGER NOT NULL DEFAULT 0;
            """,
            // Bookmarks in long files (plan 033).
            """
            CREATE TABLE bookmark (id TEXT PRIMARY KEY, song INTEGER NOT NULL, positionMs INTEGER NOT NULL,
              note TEXT NOT NULL, at INTEGER NOT NULL, deleted INTEGER NOT NULL);
            """,
        ]
        let have = sql.userVersion
        for (i, step) in steps.enumerated() where i >= have {
            try sql.transaction { try sql.exec(step) }
            sql.userVersion = i + 1
        }
    }

    /// Everything this account kept, its unsent edits included: used when the
    /// user signs out or another signs in, as the phone's `forgetAccount`. A
    /// refused token keeps everything instead (plan 023), so an offline edit
    /// is only ever dropped by the person who made it.
    public func wipe() throws {
        try sql.exec(
            """
            DELETE FROM song; DELETE FROM playlist; DELETE FROM queue; DELETE FROM mark; DELETE FROM resume;
            DELETE FROM setting; DELETE FROM outbox; DELETE FROM play_stat; DELETE FROM pin; DELETE FROM download;
            DELETE FROM lyrics; DELETE FROM bookmark;
            """)
    }

    /// A read's rows; one that fails reads as none.
    private func rows<T>(_ q: String, _ args: [SQLValue?] = [], bulk: Bool = false, _ row: (SQLite.Row) -> T)
        -> [T]
    {
        (try? sql.query(q, args, bulk: bulk, row)) ?? []
    }

    /// Writes one row, its values in the table's column order.
    private func put(_ q: String, _ values: [any SQLBindable]) { try? sql.run(q, values.sql) }

    // MARK: Songs

    public func songs() -> [Song] {
        rows("SELECT * FROM song WHERE missing = 0", bulk: true) { r in
            Song(
                id: r.int(0), path: r.text(1), title: r.text(2), artist: r.text(3), artists: split(r.text(4)),
                album: r.text(5), albumArtist: r.text(6), composer: r.text(7), genres: split(r.text(8)),
                year: r.int(9), track: r.int(10), disc: r.int(11), durationMs: r.int(12), format: r.text(13),
                bitrate: r.int(14), sampleRate: r.int(15), bitDepth: r.int(16), size: r.int(17),
                hasArt: r.bool(18), hasLyrics: r.bool(19), addedAt: r.int(20), missing: r.bool(21),
                art: r.text(22), kind: r.text(23), date: r.text(24), notes: r.text(25),
                chapters: chapters(r.text(26)), groupKey: r.text(27), groupTitle: r.text(28),
                groupIndex: r.int(29))
        }
    }

    /// In transactions of a couple of thousand rows: the first pull is a
    /// hundred thousand, and a player's resume point written on the main
    /// thread waits for the transaction in hand, not for all of them.
    public func putSongs(_ songs: [Song]) throws {
        let rows = songs.map { s in
            ([
                s.id, s.path, s.title, s.artist, s.artists.joined(separator: sep), s.album, s.albumArtist,
                s.composer, s.genres.joined(separator: sep), s.year, s.track, s.disc, s.durationMs,
                s.format,
                s.bitrate, s.sampleRate, s.bitDepth, s.size, s.hasArt, s.hasLyrics, s.addedAt, s.missing,
                s.art, s.kind, s.date, s.notes, chapters(s.chapters), s.groupKey, s.groupTitle,
                s.groupIndex,
            ] as [any SQLBindable]).sql
        }
        for start in stride(from: 0, to: rows.count, by: 2000) {
            try sql.runMany(
                "INSERT OR REPLACE INTO song VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                Array(rows[start..<min(start + 2000, rows.count)]))
        }
    }

    // MARK: Playlists

    public func playlists() -> [Playlist] {
        let q = "SELECT id, name, path, shared, songs, ref FROM playlist ORDER BY shared, name COLLATE NOCASE"
        return rows(q) { r in
            Playlist(
                id: r.int(0), name: r.text(1), path: r.text(2), shared: r.bool(3), songs: ids(r.text(4)),
                ref: r.text(5))
        }
    }

    public func putPlaylist(_ p: Playlist) {
        put(
            "INSERT OR REPLACE INTO playlist VALUES (?,?,?,?,?,?)",
            [p.id, p.name, p.path, p.shared, join(p.songs), p.ref])
    }

    public func deletePlaylist(_ id: Int) { try? sql.run("DELETE FROM playlist WHERE id = ?", [.int(id)]) }

    public func localPlaylists() -> [Playlist] { playlists().filter { $0.id < 0 } }

    // MARK: Queues

    public func queues() -> [QueueRow] {
        rows("SELECT * FROM queue ORDER BY usedAt DESC") { r in
            QueueRow(
                id: r.text(0), name: r.text(1), songs: ids(r.text(2)), currentSong: r.int(3),
                positionMs: r.int(4),
                shuffle: r.bool(5), repeatMode: r.text(6), usedAt: r.int(7))
        }
    }

    public func putQueue(_ q: QueueRow) {
        put(
            "INSERT OR REPLACE INTO queue VALUES (?,?,?,?,?,?,?,?)",
            [q.id, q.name, join(q.songs), q.currentSong, q.positionMs, q.shuffle, q.repeatMode, q.usedAt])
    }

    public func deleteQueue(_ id: String) { try? sql.run("DELETE FROM queue WHERE id = ?", [.text(id)]) }

    // MARK: Favorites, Listen Later, resume points, settings

    public func marks(_ kind: String) -> [Mark] {
        rows(
            "SELECT kind, song, at, deleted FROM mark WHERE kind = ? AND deleted = 0 ORDER BY at DESC",
            [.text(kind)]
        ) { mark($0) }
    }

    public func mark(_ kind: String, _ song: Int) -> Mark? {
        rows(
            "SELECT kind, song, at, deleted FROM mark WHERE kind = ? AND song = ?", [.text(kind), .int(song)]
        ) { mark($0) }.first
    }

    private func mark(_ r: SQLite.Row) -> Mark {
        Mark(kind: r.text(0), song: r.int(1), at: r.int(2), deleted: r.bool(3))
    }

    public func putMark(_ m: Mark) {
        put("INSERT OR REPLACE INTO mark VALUES (?,?,?,?)", [m.kind, m.song, m.at, m.deleted])
    }

    public func resumes() -> [Resume] {
        rows("SELECT song, positionMs, at, deleted FROM resume WHERE deleted = 0 ORDER BY at DESC") {
            resume($0)
        }
    }

    public func resume(_ song: Int) -> Resume? {
        rows("SELECT song, positionMs, at, deleted FROM resume WHERE song = ?", [.int(song)]) { resume($0) }
            .first
    }

    private func resume(_ r: SQLite.Row) -> Resume {
        Resume(song: r.int(0), positionMs: r.int(1), at: r.int(2), deleted: r.bool(3))
    }

    public func putResume(_ r: Resume) {
        put("INSERT OR REPLACE INTO resume VALUES (?,?,?,?)", [r.song, r.positionMs, r.at, r.deleted])
    }

    public func bookmarks() -> [Bookmark] {
        rows("SELECT * FROM bookmark WHERE deleted = 0 ORDER BY positionMs") { bookmark($0) }
    }

    public func bookmark(_ id: String) -> Bookmark? {
        rows("SELECT * FROM bookmark WHERE id = ?", [.text(id)]) { bookmark($0) }.first
    }

    private func bookmark(_ r: SQLite.Row) -> Bookmark {
        Bookmark(
            id: r.text(0), song: r.int(1), positionMs: r.int(2), note: r.text(3), at: r.int(4),
            deleted: r.bool(5))
    }

    public func putBookmark(_ b: Bookmark) {
        put(
            "INSERT OR REPLACE INTO bookmark VALUES (?,?,?,?,?,?)",
            [b.id, b.song, b.positionMs, b.note, b.at, b.deleted])
    }

    public func settings() -> [String: JSON] {
        Dictionary(
            rows("SELECT name, value FROM setting") { ($0.text(0), JSON.parse($0.text(1))) },
            uniquingKeysWith: { a, _ in a })
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
        rows("SELECT id, key, json FROM outbox ORDER BY seq LIMIT ?", [.int(limit)]) { r in
            OutboxOp(id: r.text(0), key: r.text(1), json: r.text(2))
        }
    }

    public func outboxSize() -> Int { rows("SELECT count(*) FROM outbox") { $0.int(0) }.first ?? 0 }

    /// Drops earlier ops with the same key (only the latest matters), then
    /// adds this one. Throws rather than swallowing: an edit the outbox did
    /// not take would be lost without anyone knowing (AGENTS.md).
    public func addOp(_ op: OutboxOp, replacing key: String?) throws {
        try sql.transaction {
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
        let stats = rows("SELECT song, count, lastPlayedAt FROM play_stat") {
            PlayStat(song: $0.int(0), count: $0.int(1), lastPlayedAt: $0.int(2))
        }
        return Dictionary(stats.map { ($0.song, $0) }, uniquingKeysWith: { a, _ in a })
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
        rows("SELECT key, kind, ref, name, at FROM pin ORDER BY at DESC") { r in
            Pin(key: r.text(0), kind: r.text(1), ref: r.text(2), name: r.text(3), at: r.int(4))
        }
    }

    public func putPin(_ p: Pin) {
        put("INSERT OR REPLACE INTO pin VALUES (?,?,?,?,?)", [p.key, p.kind, p.ref, p.name, p.at])
    }

    public func deletePin(_ key: String) { try? sql.run("DELETE FROM pin WHERE key = ?", [.text(key)]) }

    public func movePin(from old: String, to key: String, ref: String) {
        try? sql.run("UPDATE pin SET key = ?, ref = ? WHERE key = ?", [.text(key), .text(ref), .text(old)])
    }

    public func downloads() -> [Int: Download] {
        let downloads = rows("SELECT song, path, size, at FROM download") {
            Download(song: $0.int(0), path: $0.text(1), size: $0.int(2), at: $0.int(3))
        }
        return Dictionary(downloads.map { ($0.song, $0) }, uniquingKeysWith: { a, _ in a })
    }

    public func putDownload(_ d: Download) {
        put("INSERT OR REPLACE INTO download VALUES (?,?,?,?)", [d.song, d.path, d.size, d.at])
    }

    public func deleteDownload(_ song: Int) {
        try? sql.run("DELETE FROM download WHERE song = ?", [.int(song)])
    }

    // MARK: Lyrics and thumbnails

    public func lyrics(_ song: Int) -> String? {
        rows("SELECT text FROM lyrics WHERE song = ?", [.int(song)]) { $0.text(0) }.first
    }

    public func lyricsSongs() -> Set<Int> { Set(rows("SELECT song FROM lyrics") { $0.int(0) }) }

    /// Every song's kept lyrics, for searching them offline.
    public func allLyrics() -> [(song: Int, text: String)] {
        rows("SELECT song, text FROM lyrics") { ($0.int(0), $0.text(1)) }
    }

    public func putLyrics(_ song: Int, _ text: String) {
        put("INSERT OR REPLACE INTO lyrics VALUES (?,?,?)", [song, text, nowMs()])
    }

    public func deleteLyrics(_ song: Int) { try? sql.run("DELETE FROM lyrics WHERE song = ?", [.int(song)]) }

    public func thumb(_ key: String) -> Data? {
        rows("SELECT data FROM thumb WHERE key = ?", [.text(key)]) { $0.blob(0) }.first
    }

    public func artKeys() -> [String] {
        rows("SELECT DISTINCT art FROM song WHERE missing = 0 AND hasArt = 1 AND art != ''") { $0.text(0) }
    }

    public func thumbKeys() -> Set<String> { Set(rows("SELECT key FROM thumb") { $0.text(0) }) }

    public func putThumbs(_ thumbs: [(String, Data)]) {
        try? sql.runMany(
            "INSERT OR REPLACE INTO thumb VALUES (?,?)", thumbs.map { [.text($0.0), .blob($0.1)] })
    }
}

/// Joins and splits the text form of a list (artists, genres).
let sep = "\u{1f}"
private func split(_ s: String) -> [String] { s.isEmpty ? [] : s.components(separatedBy: sep) }
private func chapters(_ s: String) -> [Chapter] {
    s.isEmpty ? [] : (try? JSONDecoder().decode([Chapter].self, from: Data(s.utf8))) ?? []
}
private func chapters(_ c: [Chapter]) -> String {
    c.isEmpty ? "" : (try? JSONEncoder().encode(c)).map { String(decoding: $0, as: UTF8.self) } ?? ""
}
func ids(_ s: String) -> [Int] { s.isEmpty ? [] : s.split(separator: ",").compactMap { Int($0) } }
func join(_ ids: [Int]) -> String { ids.map(String.init).joined(separator: ",") }
