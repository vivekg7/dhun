import AVFoundation

/// The player: decoders → `AVAudioPlayerNode` → `AVAudioUnitTimePitch` →
/// output (plan 024). Songs are scheduled back to back on one node, so the
/// next song starts on the sample after the last one ends (gapless), and the
/// time-pitch unit changes speed and pitch independently, as Media3 does on
/// the phone.
///
/// Decoding runs ahead on its own queue, about a second of audio, and may
/// block there waiting for bytes; the render thread only ever sees buffers
/// that are ready. `play` and `seek` wait for that queue, so they are never
/// called on the main thread; a read waiting for bytes is interrupted first,
/// so a skip never waits for the network.
public final class Engine: @unchecked Sendable {
    public struct Position: Sendable {
        public let song: Int
        public let seconds: Double
        /// As the decoder measured it; 0 if unknown.
        public let duration: Double
    }

    public let format: AVAudioFormat
    /// Asked for the song after `song`, once `song` has been decoded to its
    /// end; nil ends playback there. Runs on the decoding queue.
    public var next: (@Sendable (_ song: Int) -> (song: Int, bytes: Bytes)?)?
    /// A song could not be opened or read to its end; playback goes on with the next.
    public var failed: (@Sendable (Int, Error) -> Void)?
    /// The output changed (headphones out, another device): the engine has
    /// stopped. Called on an arbitrary thread.
    public var interrupted: (@Sendable () -> Void)?

    private let engine = AVAudioEngine()
    private let node = AVAudioPlayerNode()
    private let tempo = AVAudioUnitTimePitch()
    private let queue = DispatchQueue(label: "dhun.decode")
    private let lock = NSLock()

    private struct Segment {
        let song: Int
        let decoder: SongDecoder
        let startSample: Int64  // on the node's timeline since the last play()
        let startSeconds: Double  // where in the song that sample is
    }
    // Guarded by `lock`: read by position() and play() from any thread.
    private var segments: [Segment] = []
    private var finished = false
    private var endSample: Int64 = 0
    private var reading: Bytes?

    // Owned by `queue`.
    private var feeding: Segment?
    private var scheduled: Int64 = 0
    private var outstanding = 0
    private var generation = 0
    private let chunk: AVAudioFrameCount = 8192
    private let ahead = 6
    private var observer: NSObjectProtocol?

    public init() {
        let rate = engine.outputNode.outputFormat(forBus: 0).sampleRate
        format = AVAudioFormat(standardFormatWithSampleRate: rate > 0 ? rate : 48000, channels: 2)!
        engine.attach(node)
        engine.attach(tempo)
        engine.connect(node, to: tempo, format: format)
        engine.connect(tempo, to: engine.mainMixerNode, format: format)
        observer = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil
        ) { [weak self] _ in self?.interrupted?() }
    }

    deinit { if let observer { NotificationCenter.default.removeObserver(observer) } }

    public var speed: Float {
        get { tempo.rate }
        set { tempo.rate = newValue }
    }

    /// Semitones, −6…+6 in the app (plan 016).
    public var semitones: Float {
        get { tempo.pitch / 100 }
        set { tempo.pitch = newValue * 100 }
    }

    /// For the sleep timer's fade (plan 014).
    public var volume: Float {
        get { engine.mainMixerNode.outputVolume }
        set { engine.mainMixerNode.outputVolume = newValue }
    }

    public var isPlaying: Bool { node.isPlaying }

    /// Opens `bytes` and starts it at `seconds`, dropping whatever was
    /// scheduled; `paused` leaves it ready but silent. Blocks until the first
    /// audio is decoded.
    public func play(song: Int, bytes: Bytes, from seconds: Double = 0, paused: Bool = false) throws {
        interruptReading()
        try queue.sync {
            generation += 1
            node.stop()
            lock.withLock {
                segments = []
                reading = bytes
            }
            let decoder = try openDecoder(bytes, format: format)
            if seconds > 0 { try decoder.seek(to: seconds) }
            start(Segment(song: song, decoder: decoder, startSample: 0, startSeconds: seconds))
        }
        try startNode(paused: paused)
    }

    /// Seeks within the song now playing.
    public func seek(to seconds: Double) throws {
        guard let cur = lock.withLock({ currentSegment() }) else { return }
        let playing = node.isPlaying
        interruptReading()
        try queue.sync {
            generation += 1
            node.stop()
            // On the queue: feed() may be reading this decoder.
            try cur.decoder.seek(to: max(0, seconds))
            start(
                Segment(song: cur.song, decoder: cur.decoder, startSample: 0, startSeconds: max(0, seconds)))
        }
        try startNode(paused: !playing)
    }

    public func pause() { node.pause() }

    public func resume() throws { try startNode(paused: false) }

    public func stop() {
        interruptReading()
        queue.sync {
            generation += 1
            node.stop()
            feeding = nil
            lock.withLock {
                segments = []
                reading = nil
            }
        }
    }

    /// The song being heard and where in it, or nil once everything has played.
    public func position() -> Position? {
        lock.withLock {
            guard let s = currentSegment() else { return nil }
            return Position(
                song: s.song, seconds: s.startSeconds + Double(heard() - s.startSample) / format.sampleRate,
                duration: s.decoder.duration)
        }
    }

    // MARK: Inside

    private func startNode(paused: Bool) throws {
        if !engine.isRunning { try engine.start() }
        if paused { node.pause() } else { node.play() }
    }

    /// A read waiting for bytes gives up, so the queue is free at once.
    private func interruptReading() { lock.withLock { reading }?.interrupt() }

    /// Runs on `queue`.
    private func start(_ s: Segment) {
        lock.withLock {
            segments = [s]
            finished = false
        }
        feeding = s
        scheduled = 0
        outstanding = 0
        feed()
    }

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
                // Interrupted by a play, seek or stop: whoever interrupted carries on.
                if isCancel(error) { return }
                failed?(cur.song, error)
                buf.frameLength = 0
            }
            if buf.frameLength == 0 {
                guard let s = openNext(after: cur.song) else {
                    feeding = nil
                    let end = scheduled
                    lock.withLock {
                        finished = true
                        endSample = end
                        reading = nil
                    }
                    break
                }
                if gen != generation { return }
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

    /// The next song that opens; one that cannot is reported and passed over.
    private func openNext(after song: Int) -> Segment? {
        var after = song
        while let (next, bytes) = next?(after) {
            lock.withLock { reading = bytes }
            do {
                return Segment(
                    song: next, decoder: try openDecoder(bytes, format: format), startSample: scheduled,
                    startSeconds: 0)
            } catch {
                if isCancel(error) { return nil }
                failed?(next, error)
                after = next
            }
        }
        return nil
    }

    private func isCancel(_ error: Error) -> Bool {
        if case BytesError.cancelled = error { return true }
        if let e = error as? DecodeError, e.cancelled { return true }
        return false
    }
}
