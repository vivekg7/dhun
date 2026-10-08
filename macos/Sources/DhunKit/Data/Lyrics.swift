import Foundation

/// Lyrics (plan 015), as the server stores them: a sibling `.lrc` file or
/// embedded tags. Every copy fetched is kept in the database, so the lyrics
/// of a downloaded song, or of one already viewed, show offline too.
@MainActor
public final class Lyrics {
    public enum State: Equatable, Sendable {
        case loading
        /// The server has none for this song.
        case none
        /// Not kept here, and the server cannot be reached.
        case offline
        case shown(ParsedLyrics)
    }

    private unowned let app: AppModel

    init(app: AppModel) { self.app = app }

    /// The kept copy first, then the server's when it differs.
    public func load(_ song: Song, _ update: @escaping (State) -> Void) async {
        guard song.hasLyrics else { return update(.none) }
        let kept = app.db.lyrics(song.id)
        update(kept.map { .shown(parseLyrics($0)) } ?? .loading)
        do {
            let text = try await fetch(song.id)
            if let text {
                if text != kept { update(.shown(parseLyrics(text))) }
            } else {
                update(.none)
            }
        } catch {
            // Unreachable, or an answer we cannot read: the kept copy, if any, stays up.
            if kept == nil { update(.offline) }
        }
    }

    /// Fetches and keeps a song's lyrics; nil (and forgotten) when the server has none.
    func fetch(_ song: Int) async throws -> String? {
        do {
            let text = try await app.api.lyrics(song).text ?? ""
            app.db.putLyrics(song, text)
            return text
        } catch let e as ApiError where e.code == 404 {
            app.db.deleteLyrics(song)
            return nil
        }
    }

    /// For downloads: the lyrics of `songs` not yet kept. Lyrics are small, so this runs on any network.
    func keep(_ songs: [Song]) async {
        let kept = app.db.lyricsSongs()
        for s in songs where s.hasLyrics && !kept.contains(s.id) {
            guard (try? await fetch(s.id)) != nil else { return }
        }
    }
}

/// One line; `ms` is where it starts, or -1 in lyrics without times.
public struct LyricLine: Hashable, Sendable {
    public let ms: Int
    public let text: String
}

public struct ParsedLyrics: Hashable, Sendable {
    public let synced: Bool
    public let lines: [LyricLine]

    /// The line playing at `ms`: the last one started, or -1 before the first.
    public func at(_ ms: Int) -> Int {
        guard synced else { return -1 }
        var i = -1
        for (j, l) in lines.enumerated() {
            if l.ms <= ms { i = j } else { break }
        }
        return i
    }
}

nonisolated(unsafe) private let stamp = /\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?\]/
nonisolated(unsafe) private let tagLine = /^\[([a-zA-Z#]+):(.*)\]$/
nonisolated(unsafe) private let wordStamp = /<\d{1,3}:\d{1,2}(?:[.:]\d{1,3})?>/

/// LRC by our own small parser, the phone's: `[mm:ss.xx]` stamps, several
/// on one line for a repeated chorus, `[offset:±ms]`, and word stamps
/// (`<mm:ss.xx>`, enhanced LRC), which are dropped. Text with no stamps is
/// shown as plain lyrics, without its `[ar:…]`-style tags.
public func parseLyrics(_ text: String) -> ParsedLyrics {
    var offset = 0
    var timed: [LyricLine] = []
    var plain: [String] = []
    for raw in text.split(omittingEmptySubsequences: false, whereSeparator: \.isNewline) {
        var line = Substring(raw.trimmingCharacters(in: .whitespaces))
        if let tag = line.wholeMatch(of: tagLine), line.wholeMatch(of: stamp) == nil {
            if tag.1.lowercased() == "offset" {
                offset = Int(tag.2.trimmingCharacters(in: .whitespaces)) ?? 0
            }
            continue
        }
        var times: [Int] = []
        while let m = line.prefixMatch(of: stamp) {
            let frac = String(m.3 ?? "").padding(toLength: 3, withPad: "0", startingAt: 0)
            times.append(Int(m.1)! * 60_000 + Int(m.2)! * 1000 + (Int(frac) ?? 0))
            line = line[m.range.upperBound...]
        }
        let clean = line.replacing(wordStamp, with: "").trimmingCharacters(in: .whitespaces)
        if times.isEmpty {
            plain.append(clean)
        } else {
            timed += times.map { LyricLine(ms: $0, text: clean) }
        }
    }
    if timed.isEmpty {
        // Blank lines at either end are file layout, not verses.
        while plain.first?.isEmpty == true { plain.removeFirst() }
        while plain.last?.isEmpty == true { plain.removeLast() }
        return ParsedLyrics(synced: false, lines: plain.map { LyricLine(ms: -1, text: $0) })
    }
    // A positive offset shows the lines earlier.
    let shifted = timed.map { LyricLine(ms: max(0, $0.ms - offset), text: $0.text) }
    return ParsedLyrics(
        synced: true,
        lines: shifted.enumerated().sorted { ($0.element.ms, $0.offset) < ($1.element.ms, $1.offset) }.map(
            \.element))
}
