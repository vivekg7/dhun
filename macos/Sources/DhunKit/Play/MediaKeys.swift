import AppKit
import MediaPlayer

/// Media keys, AirPods and headset buttons, Control Center and the lock
/// screen: the Mac's counterpart of the phone's media session (plan 024).
@MainActor
final class MediaKeys {
    private unowned let playback: Playback
    private var artFor: Int?

    init(playback: Playback) {
        self.playback = playback
        let c = MPRemoteCommandCenter.shared()
        c.playCommand.addTarget { [weak self] _ in self?.run { $0.play() } ?? .commandFailed }
        c.pauseCommand.addTarget { [weak self] _ in self?.run { $0.pause() } ?? .commandFailed }
        c.togglePlayPauseCommand.addTarget { [weak self] _ in self?.run { $0.toggle() } ?? .commandFailed }
        c.nextTrackCommand.addTarget { [weak self] _ in self?.run { $0.next() } ?? .commandFailed }
        c.previousTrackCommand.addTarget { [weak self] _ in self?.run { $0.previous() } ?? .commandFailed }
        c.changePlaybackPositionCommand.addTarget { [weak self] e in
            guard let e = e as? MPChangePlaybackPositionCommandEvent else { return .commandFailed }
            return self?.run { $0.seek(to: e.positionTime) } ?? .commandFailed
        }
    }

    private func run(_ f: (Playback) -> Void) -> MPRemoteCommandHandlerStatus {
        guard playback.current != nil else { return .noActionableNowPlayingItem }
        f(playback)
        update()
        return .success
    }

    /// After a new song, play or pause, or a seek.
    func update() {
        let center = MPNowPlayingInfoCenter.default()
        guard let s = playback.current else {
            center.nowPlayingInfo = nil
            center.playbackState = .stopped
            return
        }
        var info: [String: Any] = [
            MPMediaItemPropertyTitle: s.title,
            MPMediaItemPropertyArtist: s.displayArtist,
            MPMediaItemPropertyAlbumTitle: s.album,
            MPMediaItemPropertyPlaybackDuration: playback.duration,
            MPNowPlayingInfoPropertyElapsedPlaybackTime: playback.position,
            MPNowPlayingInfoPropertyPlaybackRate: playback.isPlaying ? playback.tempo.speed : 0,
        ]
        if let old = center.nowPlayingInfo?[MPMediaItemPropertyArtwork], artFor == s.id {
            info[MPMediaItemPropertyArtwork] = old
        }
        center.nowPlayingInfo = info
        center.playbackState = playback.isPlaying ? .playing : .paused
        guard artFor != s.id else { return }
        artFor = s.id
        Task {
            guard let img = await playback.app.covers.image(s, px: 512), playback.current?.id == s.id else {
                return
            }
            var i = center.nowPlayingInfo ?? [:]
            i[MPMediaItemPropertyArtwork] = artwork(img)
            center.nowPlayingInfo = i
        }
    }
}

/// Outside the main actor: MediaPlayer asks for the image on a queue of its
/// own, and a closure made on the main actor would trap there.
private nonisolated func artwork(_ img: NSImage) -> MPMediaItemArtwork {
    MPMediaItemArtwork(boundsSize: img.size) { _ in img }
}
