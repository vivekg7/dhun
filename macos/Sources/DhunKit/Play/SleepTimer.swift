import Foundation
import Observation

/// Stops playback later (plan 014), as on the phone: after some minutes, at
/// the end of this song, after a number of songs, at the end of the queue,
/// or after a song picked in the queue (plan 020). A timer by the clock
/// fades out over its last seconds and pauses on time; the others pause
/// where a song ends, which needs no fade.
@MainActor @Observable
public final class SleepTimer {
    public enum Mode: Equatable, Sendable {
        case at(Date)
        case endOfSong
        /// `left` songs to go, the playing one included.
        case songs(Int)
        case endOfQueue
        /// Musicolet's "Stop after this song", for a song further down the queue.
        case afterSong(Int, String)
    }

    public private(set) var mode: Mode?
    @ObservationIgnored private var fading: Task<Void, Never>?
    @ObservationIgnored unowned let playback: Playback

    /// How long the volume takes to fall to nothing before a timer by the clock pauses.
    static let fade: Double = 10

    init(playback: Playback) { self.playback = playback }

    public func minutes(_ m: Int) {
        let end = Date().addingTimeInterval(Double(m) * 60)
        set(.at(end))
        fading = Task { [weak self] in
            try? await Task.sleep(for: .seconds(max(0, end.timeIntervalSinceNow - SleepTimer.fade)))
            while let self, !Task.isCancelled {
                let left = end.timeIntervalSinceNow
                if left <= 0 { break }
                self.playback.engine.volume = Float(min(1, max(0, left / SleepTimer.fade)))
                try? await Task.sleep(for: .milliseconds(100))
            }
            guard let self, !Task.isCancelled else { return }
            self.playback.pause()
            self.cancel()
        }
    }

    public func endOfSong() { set(.endOfSong) }
    public func songs(_ n: Int) { set(n <= 1 ? .endOfSong : .songs(n)) }
    public func endOfQueue() { set(.endOfQueue) }
    public func afterSong(_ song: Int, _ title: String) { set(.afterSong(song, title)) }

    public func cancel() {
        fading?.cancel()
        fading = nil
        playback.engine.volume = 1
        mode = nil
        update()
    }

    private func set(_ m: Mode) {
        cancel()
        mode = m
        update()
    }

    /// A song started (on its own or skipped to): one fewer to go, and maybe this is the last.
    func onNextSong() {
        if case .songs(let left) = mode { mode = left <= 2 ? .endOfSong : .songs(left - 1) }
        update()
    }

    /// Playback paused itself at the end of the last song.
    func onPausedAtEnd() {
        if let mode, case .at = mode { return }
        if mode != nil { cancel() }
    }

    /// Tells the player to stop where the playing song ends, when it is the last one.
    func update() {
        let current = playback.current?.id
        let stop: Bool
        switch mode {
        case .afterSong(let song, _): stop = current == song
        case .endOfSong: stop = true
        case .endOfQueue: stop = !playback.hasNext
        default: stop = false
        }
        playback.pauseAfter(stop ? current : nil)
    }

    /// "in 23 min", "after this song", …, or nil with no timer.
    public var label: String? {
        switch mode {
        case nil: return nil
        case .at(let end): return "in \(max(1, Int((end.timeIntervalSinceNow + 59) / 60))) min"
        case .endOfSong: return "after this song"
        case .songs(let n): return "after \(n) songs"
        case .endOfQueue: return "at the end of the queue"
        case .afterSong(_, let title): return "after “\(title)”"
        }
    }
}
