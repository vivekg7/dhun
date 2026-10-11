import AVFoundation
import Foundation
import Testing

@testable import DhunKit

/// A seek far into an MP3 still arriving must not wait for the bytes before
/// the place. Core Audio walks every frame before it to find it, and without
/// a Xing or Info frame walks them all just to open the file, so a book
/// resumed hours in waited for hundreds of megabytes (plan 031). Here only
/// the head and the bytes from 12 s on have arrived; a read of anything else
/// fails after 2 s rather than hang. The chirp's pitch, 200 Hz rising 20 Hz a
/// second, says where the audio landed.
@Test(arguments: [true, false])
func seekFarIntoAnMp3StillArriving(withInfoFrame: Bool) throws {
    let url = Bundle.module.url(forResource: "chirp", withExtension: "mp3", subdirectory: "Fixtures")!
    var data = try Data(contentsOf: url)
    if !withInfoFrame, let info = try Mp3Layout(FileBytes(url))?.xing {
        data.removeSubrange(Int(info.lowerBound)..<Int(info.upperBound))
    }
    let path = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: path) }
    let file = try SparseFile(path, size: Int64(data.count))
    let later = 12 * 32000 / 8  // 32 kb/s
    file.write(0, data[0..<16384])
    file.write(Int64(later), data[later...])
    DispatchQueue.global().asyncAfter(deadline: .now() + 2) { file.fail(BytesError.failed("waited")) }

    let format = AVAudioFormat(standardFormatWithSampleRate: 32000, channels: 2)!
    let d = try openDecoder(file, format: format)
    #expect(abs(d.duration - 20) < 0.1)
    try d.seek(to: 15)
    let buf = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 3200)!
    try d.read(into: buf)
    let x = UnsafeBufferPointer(start: buf.floatChannelData![0], count: Int(buf.frameLength))
    let crossings = zip(x, x.dropFirst()).filter { ($0 < 0) != ($1 < 0) }.count
    let hz = Double(crossings) / 2 / (Double(x.count) / 32000)
    // 15 s to 15.1 s, with no silence before it: about 501 Hz.
    #expect(abs(hz - 501) < 10, "\(hz) Hz")
}
