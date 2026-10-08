import Foundation
import Testing

@testable import DhunKit

// Ported from the phone's tests (android/app/src/test), so the two apps keep
// the same rules (plan 024).

private func song(
    _ id: Int, _ path: String, _ title: String = "", album: String = "", artist: String = "",
    albumArtist: String = "", track: Int = 0
) -> Song {
    Song(
        id: id, path: path, title: title.isEmpty ? "Song \(id)" : title, artist: artist,
        artists: artist.isEmpty ? [] : [artist], album: album, albumArtist: albumArtist, composer: "",
        genres: [],
        year: 0, track: track, disc: 0, durationMs: 180_000, format: "mpeg", bitrate: 0, sampleRate: 0,
        bitDepth: 0,
        size: 1000, hasArt: false, hasLyrics: false, addedAt: 0, missing: false, art: "")
}

@Test func albumsWithTheSameNameByDifferentArtistsStaySeparate() {
    let c = Catalog([
        song(1, "A/Greatest Hits/01.mp3", "One", album: "Greatest Hits", artist: "Queen"),
        song(2, "B/Greatest Hits/01.mp3", "Two", album: "Greatest Hits", artist: "ABBA"),
        song(3, "A/Greatest Hits/02.mp3", "Three", album: "Greatest Hits", artist: "Queen"),
    ])
    #expect(c.albums.map(\.songs.count).sorted() == [1, 2])
}

@Test func compilationTaggedWithAnAlbumArtistIsOneAlbumAcrossFolders() {
    let c = Catalog([
        song(1, "x/01.mp3", "One", album: "Rockstar", artist: "Mohit", albumArtist: "A.R. Rahman", track: 1),
        song(2, "y/02.mp3", "Two", album: "Rockstar", artist: "Javed", albumArtist: "A.R. Rahman", track: 2),
    ])
    #expect(c.albums.count == 1)
    #expect(c.albums[0].songs.map(\.id) == [1, 2])
}

@Test func numbersSortAsNumbers() {
    #expect(
        ["Track 10", "track 2", "Track 1"].sorted { natural($0, $1) < 0 } == [
            "Track 1", "track 2", "Track 10",
        ])
}

@Test func searchIgnoresCaseAndAccents() {
    let c = Catalog([
        song(1, "a.mp3", "Halo", album: "I Am... Sasha Fierce", artist: "Beyoncé"),
        song(2, "b.mp3", "Tum Hi Ho", album: "Aashiqui 2", artist: "Arijit Singh"),
    ])
    #expect(c.search("beyonce").map(\.id) == [1])
    #expect(c.search("TUM hi").map(\.id) == [2])
}

@Test func foldersHoldTheirSubfoldersSongs() {
    let c = Catalog([song(1, "Hindi/Arijit/01.mp3"), song(2, "Hindi/02.mp3")])
    #expect(Set(c.folders["Hindi"]!.allSongs().map(\.id)) == [1, 2])
    #expect(c.folders[""]!.children.map(\.name) == ["Hindi"])
}

@Test func serverAddressGetsSchemeAndDefaultPort() {
    #expect(serverURL(" my-nas ") == "http://my-nas:8585")
    #expect(serverURL("192.168.1.50:8585/") == "http://192.168.1.50:8585")
    #expect(serverURL("https://nas.example.ts.net") == "https://nas.example.ts.net")
}

// Lines as the curation workflow's .lrc files have them: tags, a repeated
// chorus on one line, two- and three-digit fractions, word stamps.
@Test func syncedLinesAreTimedSortedAndClean() {
    let p = parseLyrics(
        """
        [ar:Someone]
        [ti:Saans]
        [00:05.5]First
        [00:10.25][00:30.250]Chorus
        [00:20.00]<00:20.00>Word <00:21.50>stamps
        [00:25.00]
        """)
    #expect(
        p.lines == [
            LyricLine(ms: 5_500, text: "First"), LyricLine(ms: 10_250, text: "Chorus"),
            LyricLine(ms: 20_000, text: "Word stamps"), LyricLine(ms: 25_000, text: ""),
            LyricLine(ms: 30_250, text: "Chorus"),
        ])
    #expect(p.at(5_000) == -1)
    #expect(p.at(19_999) == 1)
    #expect(p.at(99_000) == 4)
}

@Test func offsetShowsLinesEarlier() {
    #expect(
        parseLyrics("[offset:+500]\n[00:01.00]a\n[00:00.20]b").lines == [
            LyricLine(ms: 0, text: "b"), LyricLine(ms: 500, text: "a"),
        ])
}

// Embedded lyrics are often plain; a stray tag must not make them "synced" or show up as a verse.
@Test func plainLyricsKeepTheirLinesButNotTags() {
    let p = parseLyrics("\n[ar:Someone]\nOne\n\nTwo\n\n")
    #expect(!p.synced)
    #expect(p.lines.map(\.text) == ["One", "", "Two"])
    #expect(p.at(10_000) == -1)
}

// A Mac in German would otherwise show "1,25×".
@Test func speedsReadTheSameInEveryLocale() {
    #expect([0.75, 1, 1.25, 1.5, 2].map(Tempo.format) == ["0.75", "1", "1.25", "1.5", "2"])
}

// A song's setting comes from another device, maybe a newer app: never trusted to be in range.
@Test func aSongSettingIsReadDefensively() {
    #expect(Tempo.of(JSON.parse(#"{"speed":1.5,"semitones":-2}"#)) == Tempo(speed: 1.5, semitones: -2))
    #expect(Tempo.of(JSON.parse(#"{"speed":9,"semitones":40}"#)) == Tempo(speed: 2, semitones: 6))
    #expect(Tempo.of(JSON.parse(#"{"speed":1.25}"#)) == Tempo(speed: 1.25, semitones: 0))
    #expect(Tempo.of(.number(1.5)) == nil)
    #expect(Tempo.of(nil) == nil)
}

// The order is a synced setting: it can name queues deleted elsewhere, miss
// ones made on another device, or come from a newer app in another shape.
@MainActor @Test func theOrderNeverLosesOrInventsAQueue() {
    func q(_ id: String, _ usedAt: Int) -> QueueRow {
        QueueRow(
            id: id, name: id, songs: [], currentSong: 0, positionMs: 0, shuffle: false, repeatMode: "off",
            usedAt: usedAt)
    }
    let queues = [q("a", 30), q("b", 10), q("c", 20), q("d", 5)]
    // Named first, in order; the rest by use, oldest first, as Musicolet appends.
    #expect(
        Playback.orderOf(queues, JSON.parse(#"["c", "gone", "a", "c", 7]"#)).map(\.id) == [
            "c", "a", "d", "b",
        ])
    #expect(Playback.orderOf(queues, nil).map(\.id) == ["d", "b", "c", "a"])
    #expect(Playback.orderOf(queues, .string("a")).map(\.id) == ["d", "b", "c", "a"])
}

/// Before the catalogue loads, nothing is covered: an empty answer would delete every file.
@Test func anEmptyCatalogueCoversNothingRatherThanNoSongs() {
    let pin = Pin(key: "song:1", kind: Downloads.song, ref: "1", name: "", at: 0)
    #expect(covered([pin], Catalog([]), [], [], []) == nil)
}

/// A folder keeps its subfolders, a song in two pins is wanted once, and the newest pin comes first.
@Test func pinsBecomeSongsOnceNewestFirst() {
    let c = Catalog([
        song(1, "Hindi/90s/a.mp3"), song(2, "Hindi/90s/Live/b.mp3"), song(3, "Hindi/c.mp3"),
        song(4, "English/d.mp3"),
    ])
    func pin(_ kind: String, _ ref: String, _ at: Int) -> Pin {
        Pin(key: "\(kind):\(ref)", kind: kind, ref: ref, name: ref, at: at)
    }
    let pins = [
        pin(Downloads.folder, "Hindi/90s", 1), pin(Downloads.playlist, "7", 3),
        pin(Downloads.list, Store.fav, 2),
    ]
    let playlists = [Playlist(id: 7, name: "Mix", path: "", shared: false, songs: [4, 2, 0], ref: "")]
    let favs = [
        Mark(kind: Store.fav, song: 3, at: 0, deleted: false),
        Mark(kind: Store.fav, song: 1, at: 0, deleted: false),
    ]
    #expect(covered(pins, c, playlists, favs, [])!.map(\.id) == [4, 2, 3, 1])
}

/// An album's id holds a NUL between its name and its artist or folder; as a C
/// string it was cut there, and the album's download then matched no album.
@Test func anAlbumDownloadSurvivesTheDatabase() throws {
    let c = Catalog([song(1, "A/a.mp3", album: "Bravo", albumArtist: "Znaider")])
    let album = c.albums[0]
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: dir) }
    let db = try DB(path: dir.appendingPathComponent("t.db").path)
    db.putPin(
        Pin(
            key: Downloads.key(Downloads.album, album.id), kind: Downloads.album, ref: album.id, name: "",
            at: 0))
    #expect(covered(db.pins(), c, [], [], [])!.map(\.id) == [1])
}
