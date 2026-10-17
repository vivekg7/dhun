import Testing

@testable import DhunKit

// Podcasts and audiobooks (plan 031), ported from the phone's SpokenTest.

private func song(_ id: Int, _ path: String, kind: String = "", group: String = "", index: Int = 0) -> Song {
    Song(
        id: id, path: path, title: String(path.split(separator: "/").last!), artist: "Host",
        artists: ["Host"],
        album: "Show", albumArtist: "", composer: "", genres: ["Podcast"], year: 0, track: 0, disc: 0,
        durationMs: 600_000, format: "opus", bitrate: 0, sampleRate: 0, bitDepth: 0, size: 0, hasArt: false,
        hasLyrics: false, addedAt: 0, missing: false, art: "", kind: kind, groupKey: group,
        groupTitle: group.uppercased(), groupIndex: index)
}

// Each collection's paths are relative to its own folder, so an episode's
// folder can be named like a music folder; it must still stay out of every
// music list.
@Test func episodesStayOutOfMusicButQueuesFindThem() {
    let c = Catalog([
        song(1, "Lex/song.mp3"),
        song(2, "Lex/002.opus", kind: Song.podcast, group: "lex", index: 1),
        song(3, "Lex/001.opus", kind: Song.podcast, group: "lex", index: 0),
    ])
    #expect(c.songs.map(\.id) == [1])
    #expect(c.folders["Lex"]!.songs.map(\.id) == [1])
    // Folders shows them under a root of their own, after the music's.
    #expect(c.folders[""]!.children.map(\.path) == ["Lex", "Podcasts"])
    #expect(c.folders["Podcasts/Lex"]!.songs.map(\.id) == [3, 2])
    #expect(c.artists.allSatisfy { !$0.songs.contains(where: \.spoken) })
    #expect(c.shows.count == 1 && c.shows[0].items.map(\.id) == [3, 2])
    #expect(c.byId[2] != nil)
    #expect(c.shelfOf(c.byId[2]!)?.title == "LEX")
}

@Test func aBookCarriesOnFromItsFirstFileNotPlayed() {
    let c = Catalog(
        (1...3).map { song($0, "Dune/\($0).mp3", kind: Song.audiobook, group: "dune", index: $0) })
    let book = c.books[0]
    let heard = Heard(played: [1], left: [2: 300_000])
    #expect(heard.next(book).id == 2)
    #expect(heard.progress(book) == "50% · 15 min left")
    #expect(heard.begun(book))
    let done = Heard(played: [1, 2, 3], left: [:])
    #expect(done.next(book).id == 1)
    #expect(!done.begun(book), "finished, it starts again: Play, not Continue")
}

// Some podcast tools write the date tag without dashes.
@Test func aCompactDateReadsAsADay() {
    var s = song(1, "Show/1.mp3", kind: Song.podcast, group: "show")
    s.date = "20251230"
    #expect(s.day == "2025-12-30")
    s.date = "2025-12-30T08:00:00Z"
    #expect(s.day == "2025-12-30")
    s.date = "2025"
    #expect(s.day == "2025")
}

// Next and Previous go by file; a chapter is asked for by name (plan 009).
@Test func aChapterJumpStaysInTheFile() {
    let starts = [0, 60_000, 120_000]
    #expect(Playback.chapterBefore(starts, 90_000, window: 3000) == 60_000, "this chapter's start")
    #expect(Playback.chapterBefore(starts, 61_000, window: 3000) == 0, "just into it: the one before")
    #expect(Playback.chapterBefore(starts, 1000, window: 3000) == nil, "the first chapter's start stays")
    #expect(Playback.chapterBefore([], 90_000, window: 3000) == nil)
    // Just sought to a chapter, a little before its start: the one after, not that one again.
    #expect(Playback.chapterAfter(starts, 59_900) == 120_000)
    #expect(Playback.chapterAfter(starts, 130_000) == nil, "the last chapter stays")
}

/// Stopped mid-sentence in an episode: going on later starts the sentence again, a short pause does not.
@Test func aLongPauseGoesBackALittle() {
    let minute = 60_000
    func rewound(_ pos: Int, _ stoppedFor: Int, back: Int = 10_000) -> Int {
        Playback.rewound(pos, stoppedFor: stoppedFor, back: back, after: 5 * minute)
    }
    #expect(rewound(90_000, 6 * minute) == 80_000)
    #expect(rewound(90_000, 30_000) == 90_000, "a moment's pause")
    #expect(rewound(4000, 6 * minute) == 0, "never before the start")
    #expect(rewound(90_000, 6 * minute, back: 0) == 90_000, "turned off")
}
