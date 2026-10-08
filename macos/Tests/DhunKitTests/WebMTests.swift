import AVFoundation
import Testing

@testable import DhunKit

/// The two halves of one tone, decoded back to back, must give the tone back:
/// every sample, in place. This pins the WebM reader's pre-skip, its
/// DiscardPadding, and the 120 samples Apple's Opus decoder trims on its own
/// (plan 024), any of which shifts or gaps the join when wrong.
@Test func webmHalvesJoinIntoTheOriginalTone() throws {
    let format = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 2)!
    var out: [Float] = []
    var right: [Float] = []
    for n in 1...2 {
        let url = Bundle.module.url(
            forResource: "sweep-\(n)", withExtension: "webm", subdirectory: "Fixtures")!
        let d = try openDecoder(try FileBytes(url), format: format)
        let buf = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 4096)!
        while true {
            try d.read(into: buf)
            if buf.frameLength == 0 { break }
            out += UnsafeBufferPointer(start: buf.floatChannelData![0], count: Int(buf.frameLength))
            right += UnsafeBufferPointer(start: buf.floatChannelData![1], count: Int(buf.frameLength))
        }
    }
    #expect(out.count == 48000)
    #expect(right == out, "a mono song must play in both ears")

    // Compare with the tone itself, at the right offset and one each way:
    // the right one must fit far better (Opus is lossy, so not exactly).
    func error(_ lag: Int) -> Float {
        (20000..<28000).reduce(0) { sum, i in
            let t = Double(i + lag) / 48000
            let v = Float(0.4 * sin(2 * .pi * (200 + 300 * t) * t))
            return sum + (out[i] - v) * (out[i] - v)
        }
    }
    #expect(error(0) * 4 < error(-120))
    #expect(error(0) * 4 < error(120))
}

@Test func opusPacketLengthComesFromItsTOC() {
    #expect(opusSamples(Data([0xFC])) == 960)  // CELT 20 ms, one frame
    #expect(opusSamples(Data([0x61])) == 960)  // hybrid 10 ms, code 1: two frames
    #expect(opusSamples(Data([0xF3, 0x03])) == 1440)  // CELT 10 ms, code 3: three frames
}
