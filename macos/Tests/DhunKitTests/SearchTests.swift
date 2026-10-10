import Foundation
import Testing

@testable import DhunKit

// The phone's SearchTest (android/app/src/test), so the apps match alike (plan 029).

private func song(
    _ id: Int, _ title: String, artist: String = "", album: String = "", composer: String = "", year: Int = 0
) -> Song {
    Song(
        id: id, path: "a/\(id).mp3", title: title, artist: artist, artists: artist.isEmpty ? [] : [artist],
        album: album, albumArtist: "", composer: composer, genres: [], year: year, track: 0, disc: 0,
        durationMs: 180_000, format: "mpeg", bitrate: 0, sampleRate: 0, bitDepth: 0, size: 0, hasArt: false,
        hasLyrics: false, addedAt: 0, missing: false, art: "")
}

private let index = SearchIndex.songs([
    song(1, "Tum Hi Ho", artist: "Arijit Singh", album: "Aashiqui 2", year: 2013),
    song(2, "Live and Let Die", artist: "Wings"),
    song(3, "Love Me Do", artist: "The Beatles"),
    song(4, "दिल से", artist: "A.R. Rahman", album: "दिल से", composer: "A.R. Rahman", year: 1998),
    song(5, "Highway to Hell", artist: "AC/DC"),
    song(6, "Kabhie Kabhie", artist: "Mukesh", year: 1976),
    song(7, "Hello", artist: "Adele", album: "Tum Hi Ho Hits"),
])

private func find(_ q: String) -> [Int] { index.search(SearchQuery(q)).map(\.item.id) }

// The phone's and the server's copies read the same file.
@Test func foldsAsThePhoneAndTheServerDo() throws {
    let file = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent(
        "../../../api/search-fold.tsv")
    let lines = try String(contentsOf: file, encoding: .utf8).split(separator: "\n").filter {
        !$0.hasPrefix("#")
    }
    #expect(lines.count > 20)
    for line in lines {
        let cols = line.split(separator: "\t", omittingEmptySubsequences: false).map(String.init)
        #expect(Fold.text(cols[0]) == cols[1], "\(cols[0])")
        #expect(Fold.words(cols[0]).map(Fold.key).joined(separator: " ") == cols[2], "\(cols[0])")
    }
}

@Test func wordsMatchAcrossFieldsInAnyOrder() {
    #expect(find("arijit tum") == [1])
    #expect(find("ho tum arijit") == [1])
}

@Test func theTitleRanksAboveTheAlbum() {
    // Song 7 only has it in its album name.
    #expect(find("tum hi ho") == [1, 7])
}

@Test func typosOnlyWhenNothingBetterMatches() {
    #expect(find("arjit") == [1])
    // "love" matches a word, so "live" is not offered as a typo.
    #expect(find("love") == [3])
}

@Test func spellingAndScriptsMeet() {
    #expect(find("dil se") == [4])
    #expect(find("kabhi") == [6])
    #expect(find("acdc") == [5])
}

@Test func filtersNarrowAWordToAField() {
    #expect(find("composer:rahman") == [4])
    #expect(find("title:arijit") == [])
    #expect(find("year:1970-2000").sorted() == [4, 6])
    #expect(find("artist:\"arijit singh\" tum") == [1])
}

@Test func keptLyricsAreSearchedLineByLine() {
    let rows = [
        (song: 1, text: "[00:01.00]Hum tere bin ab reh nahi sakte\n[00:05.00]Tere bina kya wajood mera"),
        (song: 2, text: "Nothing here\nreh gaye"),
    ]
    let hits = searchKept(rows, SearchQuery("tere bina"))
    #expect(hits.map(\.song) == [1])
    #expect(hits.first?.line == "Tere bina kya wajood mera")
    // Words spread over two lines are not a match.
    #expect(searchKept(rows, SearchQuery("nothing gaye")).isEmpty)
}
