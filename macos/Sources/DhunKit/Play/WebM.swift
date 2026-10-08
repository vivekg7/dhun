import AVFoundation
import AudioToolbox

/// Opus in WebM (Matroska): what YouTube downloaders save with an `.opus`
/// name, and the one container in the library Core Audio cannot open
/// (plan 024). Reads the single audio track's packets and decodes them with
/// Apple's Opus decoder. Only what those files use is supported: one Opus
/// track, known element sizes, blocks without lacing.
final class WebMDecoder: SongDecoder {
    private let bytes: Bytes
    private let format: AVAudioFormat
    private var track: UInt64 = 0
    private var scale: Double = 1_000_000  // ns per timecode tick
    private var preSkip = 0  // 48 kHz samples the encoder adds in front
    private var segmentData: Int64 = 0  // offset of the Segment's payload
    private var firstCluster: Int64 = 0
    private var cuesAt: Int64?
    private var cues: [(seconds: Double, offset: Int64)]?
    private(set) var duration: Double = 0

    // Reading position.
    private var pos: Int64 = 0
    private var ends: [Int64] = []  // ends of the elements we are inside

    private var converter: AVAudioConverter!  // set once the header is read
    private var opus: AVAudioFormat!
    /// Output samples (engine rate) still to throw away: pre-skip at the
    /// start, the pre-roll after a seek.
    private var drop = 0
    private var pending: AVAudioPCMBuffer?
    private var ended = false
    private var fresh = true  // nothing decoded since the start or a seek

    init(_ bytes: Bytes, format: AVAudioFormat) throws {
        self.bytes = bytes
        self.format = format
        var channels: UInt32 = 2
        var head = Data()

        // EBML header, then the Segment's top-level elements up to the first Cluster.
        var (id, size, at) = try element(0)
        guard id == 0x1A45_DFA3 else { throw WebMError("not EBML") }
        (id, size, at) = try element(at + size)
        guard id == 0x1853_8067 else { throw WebMError("no Segment") }
        segmentData = at
        var p = at
        while p < bytes.size {
            let (id, size, data) = try element(p)
            switch id {
            case 0x1549_A966:  // Info
                for (cid, csize, cdata) in try children(data, size) {
                    if cid == 0x2A_D7B1 { scale = Double(try uint(cdata, csize)) }
                    if cid == 0x4489 { duration = try float(cdata, csize) }  // ticks
                }
            case 0x1654_AE6B:  // Tracks
                for (tid, tsize, tdata) in try children(data, size) where tid == 0xAE {
                    var number: UInt64 = 0
                    var codec = ""
                    var priv = Data()
                    for (cid, csize, cdata) in try children(tdata, tsize) {
                        switch cid {
                        case 0xD7: number = try uint(cdata, csize)
                        case 0x86: codec = String(decoding: try read(cdata, csize), as: UTF8.self)
                        case 0x63A2: priv = try read(cdata, csize)
                        case 0xE1:
                            for (aid, asize, adata) in try children(cdata, csize) where aid == 0x9F {
                                channels = UInt32(try uint(adata, asize))
                            }
                        default: break
                        }
                    }
                    if codec == "A_OPUS", track == 0 {
                        track = number
                        head = priv
                    }
                }
            case 0x114D_9B74:  // SeekHead: where the Cues are, often after the clusters
                for (sid, ssize, sdata) in try children(data, size) where sid == 0x4DBB {
                    var target = Data()
                    var position: Int64 = 0
                    for (cid, csize, cdata) in try children(sdata, ssize) {
                        if cid == 0x53AB { target = try read(cdata, csize) }
                        if cid == 0x53AC { position = Int64(try uint(cdata, csize)) }
                    }
                    if target == Data([0x1C, 0x53, 0xBB, 0x6B]) { cuesAt = segmentData + position }
                }
            case 0x1F43_B675:  // Cluster
                firstCluster = p
            default: break
            }
            if firstCluster != 0 { break }
            p = data + size
        }
        guard track != 0, firstCluster != 0 else { throw WebMError("no Opus track") }
        if head.count >= 12, head.starts(with: Array("OpusHead".utf8)) {
            preSkip = Int(head[10]) | Int(head[11]) << 8
        }
        duration = max(0, duration * scale / 1e9 - Double(preSkip) / 48000)

        var asbd = AudioStreamBasicDescription(
            mSampleRate: 48000, mFormatID: kAudioFormatOpus, mFormatFlags: 0,
            mBytesPerPacket: 0, mFramesPerPacket: 960, mBytesPerFrame: 0,
            mChannelsPerFrame: channels, mBitsPerChannel: 0, mReserved: 0)
        guard let opus = AVAudioFormat(streamDescription: &asbd),
            let converter = AVAudioConverter(from: opus, to: format)
        else { throw WebMError("no Opus decoder") }
        self.opus = opus
        self.converter = converter
        start(at: firstCluster)
        drop = Int(Double(preSkip) * format.sampleRate / 48000)
    }

    func read(into buffer: AVAudioPCMBuffer) throws {
        buffer.frameLength = 0
        while buffer.frameLength < buffer.frameCapacity, !ended || pending != nil {
            if pending == nil { pending = try decodeNext() }
            guard let p = pending else { break }
            let skip = min(drop, Int(p.frameLength))
            drop -= skip
            let n = min(Int(p.frameLength) - skip, Int(buffer.frameCapacity - buffer.frameLength))
            for ch in 0..<Int(format.channelCount) {
                (buffer.floatChannelData![ch] + Int(buffer.frameLength))
                    .update(from: p.floatChannelData![ch] + skip, count: n)
            }
            buffer.frameLength += AVAudioFrameCount(n)
            if skip + n == Int(p.frameLength) {
                pending = nil
            } else {  // keep the rest for the next read
                let rest = Int(p.frameLength) - skip - n
                for ch in 0..<Int(format.channelCount) {
                    p.floatChannelData![ch].update(from: p.floatChannelData![ch] + skip + n, count: rest)
                }
                p.frameLength = AVAudioFrameCount(rest)
            }
        }
    }

    func seek(to seconds: Double) throws {
        let target = seconds + Double(preSkip) / 48000
        let from = max(0, target - 0.08)  // Opus needs 80 ms of pre-roll
        var cluster = firstCluster
        for c in try cueList() where c.seconds <= from { cluster = c.offset }
        // Walk clusters from there to the last that starts before `from`.
        var p = cluster
        while p < bytes.size {
            let (id, size, data) = try element(p)
            guard id == 0x1F43_B675 else { break }
            let (tid, tsize, tdata) = try element(data)
            guard tid == 0xE7 else { break }
            if Double(try uint(tdata, tsize)) * scale / 1e9 > from { break }
            cluster = p
            p = data + size
        }
        converter.reset()
        fresh = true
        pending = nil
        start(at: cluster)
        // Decode-and-drop up to the target, counted from the cluster's start.
        let (_, _, data) = try element(cluster)
        let (_, tsize, tdata) = try element(data)
        let clusterStart = Double(try uint(tdata, tsize)) * scale / 1e9
        drop = Int(max(0, target - clusterStart) * format.sampleRate)
    }

    // MARK: Packets

    private func start(at cluster: Int64) {
        pos = cluster
        ends = []
        ended = false
    }

    /// The next Opus packet of our track, and the nanoseconds of padding to
    /// cut from its end (the last packet's DiscardPadding); nil at the end.
    private func nextPacket() throws -> (Data, Int64)? {
        while true {
            while let e = ends.last, pos >= e { ends.removeLast() }
            guard pos < bytes.size else { return nil }
            let (id, size, data) = try element(pos)
            pos = data + size
            switch id {
            case 0x1F43_B675:  // Cluster: step inside
                ends.append(data + size)
                pos = data
            case 0xA3:  // SimpleBlock
                if let p = try packet(data, size) { return (p, 0) }
            case 0xA0:  // BlockGroup: a Block, maybe with DiscardPadding
                var p: Data?
                var padding: Int64 = 0
                for (cid, csize, cdata) in try children(data, size) {
                    if cid == 0xA1 { p = try packet(cdata, csize) }
                    if cid == 0x75A2 {  // signed
                        let raw = try uint(cdata, csize)
                        let shift = UInt64(64 - 8 * csize)
                        padding = Int64(bitPattern: raw << shift) >> Int64(shift)
                    }
                }
                if let p { return (p, padding) }
            default:
                break
            }
        }
    }

    /// A block's frame if it belongs to our track.
    private func packet(_ offset: Int64, _ size: Int64) throws -> Data? {
        let block = try read(offset, size)
        var i = 0
        let (number, len) = vint(block, &i)
        guard len > 0, number == track else { return nil }
        guard block.count >= i + 3 else { throw WebMError("short block") }
        let flags = block[block.startIndex + i + 2]
        guard flags & 0x06 == 0 else { throw WebMError("laced blocks") }
        return block.subdata(in: block.startIndex + i + 3..<block.endIndex)
    }

    private func decodeNext() throws -> AVAudioPCMBuffer? {
        guard let (packet, padding) = try nextPacket() else {
            ended = true
            return nil
        }
        let input = AVAudioCompressedBuffer(
            format: opus, packetCapacity: 1, maximumPacketSize: packet.count)
        packet.withUnsafeBytes { input.data.copyMemory(from: $0.baseAddress!, byteCount: packet.count) }
        input.byteLength = UInt32(packet.count)
        input.packetCount = 1
        input.packetDescriptions![0] = AudioStreamPacketDescription(
            mStartOffset: 0, mVariableFramesInPacket: 0, mDataByteSize: UInt32(packet.count))
        // 120 ms is the longest Opus packet.
        let out = AVAudioPCMBuffer(
            pcmFormat: format, frameCapacity: AVAudioFrameCount(0.12 * format.sampleRate) + 64)!
        var given = false
        var error: NSError?
        let status = converter.convert(to: out, error: &error) { _, status in
            if given {
                status.pointee = .noDataNow
                return nil
            }
            given = true
            status.pointee = .haveData
            return input
        }
        if status == .error { throw error ?? WebMError("Opus decode") }
        if fresh {
            // Apple's decoder trims some of the stream's pre-skip itself (120
            // samples on macOS 27, ignoring primeMethod), so only the rest is
            // ours to drop. Measured, not assumed: what the first packet
            // should have given, less what it gave.
            fresh = false
            let expected = Int(Double(opusSamples(packet)) * format.sampleRate / 48000)
            drop = max(0, drop - max(0, expected - Int(out.frameLength)))
        }
        let cut = AVAudioFrameCount(Double(padding) / 1e9 * format.sampleRate)
        out.frameLength -= min(cut, out.frameLength)
        return out
    }

    private func cueList() throws -> [(seconds: Double, offset: Int64)] {
        if let cues { return cues }
        var list: [(Double, Int64)] = []
        if let at = cuesAt, case let (id, size, data) = try element(at), id == 0x1C53_BB6B {
            for (pid, psize, pdata) in try children(data, size) where pid == 0xBB {
                var time: Double = 0
                var offset: Int64 = -1
                for (cid, csize, cdata) in try children(pdata, psize) {
                    if cid == 0xB3 { time = Double(try uint(cdata, csize)) * scale / 1e9 }
                    if cid == 0xB7 {
                        for (kid, ksize, kdata) in try children(cdata, csize) where kid == 0xF1 {
                            offset = segmentData + Int64(try uint(kdata, ksize))
                        }
                    }
                }
                if offset >= 0 { list.append((time, offset)) }
            }
        }
        cues = list
        return list
    }

    // MARK: EBML

    /// The element at `offset`: its ID, payload size and payload offset.
    private func element(_ offset: Int64) throws -> (UInt64, Int64, Int64) {
        guard offset < bytes.size else { throw WebMError("truncated at \(offset)") }
        let head = try read(offset, min(12, bytes.size - offset))
        var i = 0
        let (id, idLen) = vint(head, &i, keepMarker: true)
        let (size, sizeLen) = vint(head, &i)
        guard idLen > 0, sizeLen > 0 else { throw WebMError("bad element at \(offset)") }
        guard size != (1 << (7 * UInt64(sizeLen))) - 1 else { throw WebMError("unknown size") }
        return (id, Int64(size), offset + Int64(i))
    }

    private func children(_ offset: Int64, _ size: Int64) throws -> [(UInt64, Int64, Int64)] {
        var out: [(UInt64, Int64, Int64)] = []
        var p = offset
        while p < offset + size {
            let e = try element(p)
            out.append(e)
            p = e.2 + e.1
        }
        return out
    }

    private func read(_ offset: Int64, _ count: Int64) throws -> Data {
        var d = Data(count: Int(count))
        let n = try d.withUnsafeMutableBytes { try bytes.read(offset, Int(count), into: $0.baseAddress!) }
        d.count = n
        return d
    }

    private func uint(_ offset: Int64, _ size: Int64) throws -> UInt64 {
        try read(offset, size).reduce(0) { $0 << 8 | UInt64($1) }
    }

    private func float(_ offset: Int64, _ size: Int64) throws -> Double {
        let bits = try uint(offset, size)
        return size == 4 ? Double(Float(bitPattern: UInt32(bits))) : Double(bitPattern: bits)
    }
}

/// Samples (48 kHz) in one Opus packet, from its TOC byte (RFC 6716, 3.1).
func opusSamples(_ packet: Data) -> Int {
    guard let toc = packet.first else { return 0 }
    let config = Int(toc >> 3)
    let size: Int  // in units of 2.5 ms, 120 samples
    switch config {
    case 0..<12: size = [4, 8, 16, 24][config & 3]
    case 12..<16: size = [4, 8][config & 1]
    default: size = [1, 2, 4, 8][config & 3]
    }
    let frames: Int
    switch toc & 3 {
    case 0: frames = 1
    case 3: frames = packet.count > 1 ? Int(packet[packet.startIndex + 1] & 0x3F) : 0
    default: frames = 2
    }
    return frames * size * 120
}

/// An EBML variable-length integer at `i`; advances `i`. Length 0 if malformed.
func vint(_ d: Data, _ i: inout Int, keepMarker: Bool = false) -> (UInt64, Int) {
    guard i < d.count else { return (0, 0) }
    let first = d[d.startIndex + i]
    let len = first.leadingZeroBitCount + 1
    guard len <= 8, i + len <= d.count else { return (0, 0) }
    var v = UInt64(keepMarker ? first : first & (0xFF >> len))
    for k in 1..<len { v = v << 8 | UInt64(d[d.startIndex + i + k]) }
    i += len
    return (v, len)
}

struct WebMError: Error, CustomStringConvertible {
    let description: String
    init(_ d: String) { description = "WebM: \(d)" }
}
