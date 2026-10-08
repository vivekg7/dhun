import AVFoundation
import Testing

@testable import DhunKit

/// One sine wave cut in two at a sample no codec frame lines up with, each half
/// encoded on its own. Decoded back to back, the join must be as smooth as the
/// wave: encoder delay or padding left in shows up as a jump or a silence.
/// The mono case also checks a mono song reaches both ears.
@Test(arguments: [("m4a", 2), ("flac", 2), ("flac", 1)])
func backToBackSongsJoinWithoutAGap(ext: String, channels: Int) throws {
    let rate = 44100.0
    let cut = 100_003
    let total = 200_000
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: dir) }

    let settings: [String: Any] =
        ext == "m4a"
        ? [
            AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: rate, AVNumberOfChannelsKey: channels,
            AVEncoderBitRateKey: 256_000,
        ]
        : [AVFormatIDKey: kAudioFormatFLAC, AVSampleRateKey: rate, AVNumberOfChannelsKey: channels]
    let pcm = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 2)!
    var halves: [URL] = []
    for (n, range) in [0..<cut, cut..<total].enumerated() {
        let url = dir.appendingPathComponent("\(n).\(ext)")
        let file = try AVAudioFile(forWriting: url, settings: settings)
        let buf = AVAudioPCMBuffer(
            pcmFormat: file.processingFormat, frameCapacity: AVAudioFrameCount(range.count))!
        buf.frameLength = buf.frameCapacity
        for (k, i) in range.enumerated() {
            let v = Float(0.5 * sin(2 * .pi * 441 * Double(i) / rate))
            for ch in 0..<channels { buf.floatChannelData![ch][k] = v }
        }
        try file.write(from: buf)
        halves.append(url)
    }

    // Decode at the file's own rate so no resampling blurs the measurement.
    var out: [Float] = []
    var right: [Float] = []
    for url in halves {
        let d = try openDecoder(try FileBytes(url), format: pcm)
        let buf = AVAudioPCMBuffer(pcmFormat: pcm, frameCapacity: 4096)!
        while true {
            try d.read(into: buf)
            if buf.frameLength == 0 { break }
            out += UnsafeBufferPointer(start: buf.floatChannelData![0], count: Int(buf.frameLength))
            right += UnsafeBufferPointer(start: buf.floatChannelData![1], count: Int(buf.frameLength))
        }
    }

    #expect(out.count == total)
    #expect(right == out)
    // A 441 Hz sine at 0.5 moves at most 2π·441/44100·0.5 ≈ 0.031 per sample.
    let join = abs(out[cut] - out[cut - 1])
    #expect(join < 0.05, "jump of \(join) at the join")
}
