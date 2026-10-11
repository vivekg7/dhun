import AVFoundation
import AudioToolbox

/// An MP3 that plays and seeks without the bytes it does not need.
///
/// Core Audio finds a place in an MP3 by walking every frame before it, so a
/// seek far into a file still arriving waited for all of it: Dune resumed at
/// 6 h after 170 MB, a minute over Tailscale (plan 031). While the bytes
/// before the place are missing, this estimates where it is, as Media3 on the
/// phone does (the Xing header's table of contents, or the bitrate), and
/// decodes from the first frame there. Once they have arrived, the seek is
/// Core Audio's own and exact.
///
/// Core Audio reads an MP3's length from its Xing or Info frame, and without
/// one walks the whole file to count its frames before it plays at all. So a
/// file without one, and every slice of a file, is opened behind such a frame:
/// the file's own, or one made up from the bitrate.
final class Mp3Decoder: SongDecoder {
    private let bytes: Bytes
    private let format: AVAudioFormat
    private let layout: Mp3Layout
    private let full: AudioFileDecoder
    private var slice: AudioFileDecoder?

    init?(_ bytes: Bytes, format: AVAudioFormat) throws {
        guard let layout = try Mp3Layout(bytes) else { return nil }
        self.bytes = bytes
        self.format = format
        self.layout = layout
        full =
            layout.xing != nil
            ? try AudioFileDecoder(bytes, format: format)
            : try Mp3Decoder.open(bytes, from: layout.audio, layout, format)
    }

    var duration: Double { full.duration }

    func read(into buffer: AVAudioPCMBuffer) throws { try (slice ?? full).read(into: buffer) }

    func seek(to seconds: Double) throws {
        // A frame's audio may begin in the frames before it (the bit
        // reservoir), so a slice starts ten frames early and decodes its way in.
        let early = max(0, seconds - 10 * layout.header.seconds)
        let guess = layout.offset(at: early, duration: duration)
        if seconds > 0, bytes.arrived(from: 0) < guess,
            let start = try Mp3Layout.frame(after: guess, in: bytes, like: layout.header)?.at
        {
            slice = try Mp3Decoder.open(bytes, from: start, layout, format)
            try slice!.seek(to: seconds - early)
        } else {
            slice = nil
            try full.seek(to: seconds)
        }
    }

    private static func open(_ bytes: Bytes, from start: Int64, _ layout: Mp3Layout, _ format: AVAudioFormat)
        throws -> AudioFileDecoder
    {
        try AudioFileDecoder(
            SliceBytes(bytes, from: start, after: layout.lengthFrame), format: format, hint: kAudioFileMP3Type
        )
    }
}

/// Where an MP3's audio is, read from its first frame.
struct Mp3Layout {
    /// The first audio frame's header, which every later one shares but for
    /// its bitrate. A Xing or Info frame's bitrate is only what fits its tag.
    let header: Mp3Header
    /// Where the first audio frame is, past the tags and the Xing frame.
    let audio: Int64
    /// The file's Xing or Info frame, which holds no audio; "Info" marks a constant bitrate.
    let xing: Range<Int64>?
    /// From a Xing frame: the stream's bytes and its table of contents.
    let toc: (bytes: Int64, entries: [UInt8])?
    /// The Xing or Info frame to open a slice behind: the file's own, or one made up.
    let lengthFrame: [UInt8]

    init?(_ bytes: Bytes) throws {
        var head = [UInt8](repeating: 0, count: 10)
        guard try bytes.read(0, 10, into: &head) == 10 else { return nil }
        var at: Int64 = 0
        if head[0...2] == [0x49, 0x44, 0x33] {  // "ID3", its size in 7-bit bytes
            let n = head[6...9].reduce(0) { $0 << 7 | Int64($1 & 0x7F) }
            at = 10 + n + (head[5] & 0x10 != 0 ? 10 : 0)
        } else if Mp3Header(head, 0) == nil {
            return nil  // not an MP3: those start with a tag or a frame
        }
        guard let first = try Mp3Layout.frame(after: at, in: bytes, like: nil) else { return nil }
        var frame = [UInt8](repeating: 0, count: first.header.length)
        let got = try bytes.read(first.at, frame.count, into: &frame)
        let p = 4 + first.header.sideInfo
        let tag = got >= p + 8 ? String(decoding: frame[p..<p + 4], as: UTF8.self) : ""
        guard tag == "Xing" || tag == "Info" else {
            header = first.header
            audio = first.at
            xing = nil
            toc = nil
            lengthFrame = header.infoFrame(stream: bytes.size - audio)
            return
        }
        audio = first.at + Int64(first.header.length)
        header = try Mp3Layout.frame(after: audio, in: bytes, like: first.header)?.header ?? first.header
        xing = first.at..<audio
        lengthFrame = frame
        let flags = frame[p + 7]
        var q = p + 8 + (flags & 1 != 0 ? 4 : 0)
        var stream = bytes.size - first.at
        if flags & 2 != 0, got >= q + 4 {
            stream = frame[q..<q + 4].reduce(0) { $0 << 8 | Int64($1) }
            q += 4
        }
        toc = tag == "Xing" && flags & 4 != 0 && got >= q + 100 ? (stream, Array(frame[q..<q + 100])) : nil
    }

    /// The byte at `seconds`: from the table of contents, read as Media3 reads
    /// it, or from the bitrate.
    func offset(at seconds: Double, duration: Double) -> Int64 {
        if let toc, let xing, duration > 0 {
            let percent = min(max(seconds / duration * 100, 0), 100)
            let a = min(Int(percent), 99)
            let lo = Double(toc.entries[a])
            let hi = a < 99 ? Double(toc.entries[a + 1]) : 256
            let scaled = lo + (hi - lo) * (percent - Double(a))
            let at = Int64((scaled / 256 * Double(toc.bytes)).rounded())
            return xing.lowerBound + min(max(at, Int64(xing.count)), toc.bytes - 1)
        }
        let frames = (seconds * Double(header.rate) / Double(header.samples)).rounded(.down)
        return audio + Int64(frames * header.meanLength)
    }

    /// Looks from `offset` for two frames in a row, the second like the first
    /// (and like `like`), so a stray sync in the audio is not taken for one:
    /// through 8 KB, which holds two of the largest, then 64 KB for junk.
    static func frame(after offset: Int64, in bytes: Bytes, like: Mp3Header?) throws -> (
        at: Int64, header: Mp3Header
    )? {
        for window in [8 << 10, 64 << 10] {
            var buf = [UInt8](repeating: 0, count: window)
            let n = offset < bytes.size ? try bytes.read(offset, window, into: &buf) : 0
            var i = 0
            while i + 4 <= n {
                if let h = Mp3Header(buf, i), like?.matches(h) ?? true {
                    let j = i + h.length
                    if j + 4 > n || Mp3Header(buf, j).map(h.matches) == true {
                        return (offset + Int64(i), h)
                    }
                }
                i += 1
            }
            if n < window { break }
        }
        return nil
    }
}

/// An MPEG audio layer III frame header.
struct Mp3Header {
    let bytes: [UInt8]
    let version: UInt8  // 3: MPEG-1, 2: MPEG-2, 0: MPEG-2.5
    let mono: Bool
    let rate: Int
    let bitrate: Int  // bits per second
    let length: Int  // the frame, in bytes
    var samples: Int { version == 3 ? 1152 : 576 }
    /// A frame's length in time.
    var seconds: Double { Double(samples) / Double(rate) }
    /// A frame's length without the odd byte of padding.
    var meanLength: Double { Double(samples * bitrate / 8) / Double(rate) }
    /// The side information's size, which a Xing header follows.
    var sideInfo: Int { version == 3 ? (mono ? 17 : 32) : (mono ? 9 : 17) }

    init?(_ b: [UInt8], _ i: Int) {
        guard b[i] == 0xFF, b[i + 1] & 0xE0 == 0xE0 else { return nil }
        version = (b[i + 1] >> 3) & 3
        let layer = (b[i + 1] >> 1) & 3
        let br = Int(b[i + 2] >> 4)
        let sr = Int((b[i + 2] >> 2) & 3)
        guard version != 1, layer == 1, br != 0, br != 15, sr != 3 else { return nil }
        let kbps =
            version == 3
            ? [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320][br]
            : [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160][br]
        bytes = Array(b[i..<i + 4])
        rate = [44100, 48000, 32000][sr] >> (version == 3 ? 0 : version == 2 ? 1 : 2)
        bitrate = kbps * 1000
        mono = b[i + 3] >> 6 == 3
        let padding = Int((b[i + 2] >> 1) & 1)
        length = (version == 3 ? 144 : 72) * bitrate / rate + padding
    }

    func matches(_ o: Mp3Header) -> Bool { o.version == version && o.rate == rate && o.mono == mono }

    /// An Info frame, as an encoder writes for a constant bitrate, for a
    /// stream of `stream` bytes after it. Core Audio wants the bytes as well
    /// as the frames, or counts the frames itself.
    func infoFrame(stream: Int64) -> [UInt8] {
        var f = [UInt8](repeating: 0, count: Int(meanLength))
        f[0...3] = bytes[0...3]
        f[1] |= 1  // no CRC
        f[2] &= ~2  // no padding
        let p = 4 + sideInfo
        f[p..<p + 8] = [0x49, 0x6E, 0x66, 0x6F, 0, 0, 0, 3]  // "Info", then frames and bytes
        let frames = Int64(Double(stream) / meanLength)
        for (n, v) in [frames, stream + Int64(f.count)].enumerated() {
            for k in 0..<4 { f[p + 8 + 4 * n + k] = UInt8(truncatingIfNeeded: v >> (24 - 8 * k)) }
        }
        return f
    }
}

/// The bytes from `start` on, as a file of their own, after the bytes in `head`.
final class SliceBytes: Bytes, @unchecked Sendable {
    private let bytes: Bytes
    private let start: Int64
    private let head: [UInt8]
    var size: Int64 { Int64(head.count) + bytes.size - start }

    init(_ bytes: Bytes, from start: Int64, after head: [UInt8]) {
        self.bytes = bytes
        self.start = start
        self.head = head
    }

    func read(_ offset: Int64, _ count: Int, into: UnsafeMutableRawPointer) throws -> Int {
        let h = Int64(head.count)
        guard offset < h else { return try bytes.read(start + offset - h, count, into: into) }
        let n = min(count, Int(h - offset))
        head.withUnsafeBytes { into.copyMemory(from: $0.baseAddress! + Int(offset), byteCount: n) }
        guard n < count else { return n }
        return try n + bytes.read(start, count - n, into: into + n)
    }

    func interrupt() { bytes.interrupt() }
}
