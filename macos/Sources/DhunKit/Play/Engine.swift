import AVFoundation

/// The player: decoders → `AVAudioPlayerNode` → `AVAudioUnitTimePitch` →
/// output (plan 024). Songs are scheduled back to back on one node, so the
/// next song starts on the sample after the last one ends (gapless), and the
/// time-pitch unit changes speed and pitch independently, as Media3 does on
/// the phone.
///
/// Decoding runs ahead on its own queue, about a second of audio, and may
/// block there waiting for bytes; the render thread only ever sees buffers
/// that are ready.
public final class Engine: @unchecked Sendable {
    public struct Position: Sendable {
        public let song: Int
        public let seconds: Double
    }

    public let format: AVAudioFormat
    /// Asked for the song after the one being decoded; nil ends playback.
    public var next: (@Sendable () -> (song: Int, decoder: Decoder)?)?
    /// Called on the decoding queue with an error that stopped a song.
    public var failed: (@Sendable (Int, Error) -> Void)?

    private let engine = AVAudioEngine()
    private let node = AVAudioPlayerNode()
    private let tempo = AVAudioUnitTimePitch()
    private let queue = DispatchQueue(label: "dhun.decode")
    private let lock = NSLock()

    private struct Segment {
        let song: Int
        let decoder: Decoder
        let startSample: Int64  // on the node's timeline since the last play()
        let startSeconds: Double  // where in the song that sample is
    }
    // Guarded by `lock`: read by position() from any thread.
    private var segments: [Segment] = []
    private var finished = false
    private var endSample: Int64 = 0

    // Owned by `queue`.
    private var feeding: Segment?
    private var scheduled: Int64 = 0
    private var outstanding = 0
    private var generation = 0
    private let chunk: AVAudioFrameCount = 8192
    private let ahead = 6

    public init() {
        let rate = engine.outputNode.outputFormat(forBus: 0).sampleRate
        format = AVAudioFormat(standardFormatWithSampleRate: rate > 0 ? rate : 48000, channels: 2)!
        engine.attach(node)
        engine.attach(tempo)
        engine.connect(node, to: tempo, format: format)
        engine.connect(tempo, to: engine.mainMixerNode, format: format)
    }

    public var speed: Float {
        get { tempo.rate }
        set { tempo.rate = newValue }
    }

    /// Semitones, −6…+6 in the app (plan 016).
    public var semitones: Float {
        get { tempo.pitch / 100 }
        set { tempo.pitch = newValue * 100 }
    }

    public var isPlaying: Bool { node.isPlaying }

    /// Starts `decoder` at `seconds`, dropping whatever was scheduled.
    public func play(song: Int, decoder: Decoder, from seconds: Double = 0) throws {
        try queue.sync {
            generation += 1
            node.stop()
            // On the queue: feed() may be reading this decoder (a seek).
            if seconds > 0 { try decoder.seek(to: seconds) }
            let s = Segment(song: song, decoder: decoder, startSample: 0, startSeconds: seconds)
            lock.withLock {
                segments = [s]
                finished = false
            }
            feeding = s
            scheduled = 0
            outstanding = 0
            feed()
        }
        if !engine.isRunning { try engine.start() }
        node.play()
    }

    public func pause() { node.pause() }

    public func resume() throws {
        if !engine.isRunning { try engine.start() }
        node.play()
    }

    public func stop() {
        queue.sync {
            generation += 1
            node.stop()
            feeding = nil
            lock.withLock { segments = [] }
        }
    }

    /// Seeks within the song now playing.
    public func seek(to seconds: Double) throws {
        guard let cur = lock.withLock({ currentSegment() }) else { return }
        let playing = node.isPlaying
        try play(song: cur.song, decoder: cur.decoder, from: seconds)
        if !playing { node.pause() }
    }

    /// The song being heard and where in it, or nil once everything has played.
    public func position() -> Position? {
        lock.withLock {
            guard let s = currentSegment() else { return nil }
            return Position(
                song: s.song, seconds: s.startSeconds + Double(heard() - s.startSample) / format.sampleRate)
        }
    }

    // MARK: Inside

    /// Samples the node has played since the last play(); call under `lock`.
    private func heard() -> Int64 {
        guard let t = node.lastRenderTime, let p = node.playerTime(forNodeTime: t) else { return 0 }
        return max(0, p.sampleTime)
    }

    /// Drops segments the node has played past; call under `lock`.
    private func currentSegment() -> Segment? {
        let at = heard()
        while segments.count > 1, segments[1].startSample <= at { segments.removeFirst() }
        if finished, segments.count == 1, at >= endSample { return nil }
        return segments.first
    }

    /// Keeps about `ahead` buffers scheduled. Runs on `queue`.
    private func feed() {
        let gen = generation
        while outstanding < ahead, let cur = feeding {
            let buf = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: chunk)!
            do {
                try cur.decoder.read(into: buf)
            } catch {
                failed?(cur.song, error)
                buf.frameLength = 0
            }
            if buf.frameLength == 0 {
                guard let (song, decoder) = next?() else {
                    feeding = nil
                    let end = scheduled
                    lock.withLock {
                        finished = true
                        endSample = end
                    }
                    break
                }
                let s = Segment(song: song, decoder: decoder, startSample: scheduled, startSeconds: 0)
                feeding = s
                lock.withLock { segments.append(s) }
                continue
            }
            scheduled += Int64(buf.frameLength)
            outstanding += 1
            node.scheduleBuffer(buf, completionCallbackType: .dataConsumed) { [weak self] _ in
                guard let self else { return }
                self.queue.async {
                    guard gen == self.generation else { return }
                    self.outstanding -= 1
                    self.feed()
                }
            }
        }
    }
}
