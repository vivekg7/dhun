import Foundation

/// What the user has heard of the spoken files (plan 031): those marked
/// played, and where the others were left.
public struct Heard: Sendable {
    public let played: Set<Int>
    let left: [Int: Int]

    public init(played: Set<Int>, left: [Int: Int]) {
        self.played = played
        self.left = left
    }

    public func heardMs(_ s: Song) -> Int {
        played.contains(s.id) ? s.durationMs : min(max(left[s.id] ?? 0, 0), s.durationMs)
    }

    /// Where a book carries on: its first file not played, or from the start once all are.
    public func next(_ shelf: Shelf) -> Song {
        shelf.items.first { !played.contains($0.id) } ?? shelf.items[0]
    }

    public func finished(_ shelf: Shelf) -> Bool { shelf.items.allSatisfy { played.contains($0.id) } }

    /// Begun and not finished: its Play button says Continue. Finished, it starts again from the first.
    public func begun(_ shelf: Shelf) -> Bool { shelf.items.contains { heardMs($0) > 0 } && !finished(shelf) }

    /// A book's progress, or its length before it is begun.
    public func progress(_ shelf: Shelf) -> String {
        let total = shelf.items.reduce(0) { $0 + $1.durationMs }
        let heard = shelf.items.reduce(0) { $0 + heardMs($1) }
        if finished(shelf) { return "Finished" }
        if heard <= 0 || total <= 0 { return length(total) }
        return "\(heard * 100 / total)% · \(length(total - heard)) left"
    }

    /// A file's line beside its title: played, or how much is left of one begun.
    public func state(_ s: Song) -> String {
        if played.contains(s.id) { return "Played" }
        let h = heardMs(s)
        return h > 0 ? "\(length(s.durationMs - h)) left" : ""
    }
}

/// "48 min", "12 h 5 min"
public func length(_ ms: Int) -> String {
    let min = ms / 60_000
    return min >= 60 ? "\(min / 60) h \(min % 60) min" : "\(min) min"
}

extension AppModel {
    public var heard: Heard {
        Heard(
            played: played,
            left: Dictionary(resumes.map { ($0.song, $0.positionMs) }, uniquingKeysWith: { a, _ in a }))
    }
}
