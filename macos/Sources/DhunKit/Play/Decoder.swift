import AVFoundation
import AudioToolbox

/// Turns one song's bytes into PCM in the engine's format.
public protocol Decoder: AnyObject {
    /// Song length in seconds, gapless trim applied; 0 if unknown.
    var duration: Double { get }
    /// Fills `buffer` (engine format) up to its capacity. A `frameLength`
    /// of 0 means the song has ended.
    func read(into buffer: AVAudioPCMBuffer) throws
    func seek(to seconds: Double) throws
}

/// Picks the decoder by the file's first bytes, never its name: many of the
/// library's `.opus` files are really WebM (plan 024).
public func openDecoder(_ bytes: Bytes, format: AVAudioFormat) throws -> Decoder {
    var magic = [UInt8](repeating: 0, count: 4)
    _ = try bytes.read(0, 4, into: &magic)
    if magic == [0x1A, 0x45, 0xDF, 0xA3] {
        return try WebMDecoder(bytes, format: format)
    }
    return try AudioFileDecoder(bytes, format: format)
}

public struct DecodeError: Error, CustomStringConvertible {
    public let description: String
    init(_ what: String, _ status: OSStatus) {
        description = "\(what) failed: \(status) '\(fourCC(status))'"
    }
}

private func fourCC(_ s: OSStatus) -> String {
    let n = UInt32(bitPattern: s)
    let b = [24, 16, 8, 0].map { UInt8((n >> UInt32($0)) & 0xFF) }
    return b.allSatisfy { $0 >= 32 && $0 < 127 } ? String(decoding: b, as: UTF8.self) : ""
}

/// Everything Core Audio reads: MP3, AAC/ALAC in MP4, FLAC, Ogg Opus, WAV,
/// AIFF. Opened through our read callbacks, so a file still arriving plays
/// the same way as one on disk. ExtAudioFile applies the encoder delay and
/// padding the file declares (LAME header, iTunSMPB, Opus pre-skip), which is
/// what makes back-to-back songs gapless, and resamples to the engine's rate.
final class AudioFileDecoder: Decoder {
    private let bytes: Bytes
    private var file: AudioFileID?
    private var ext: ExtAudioFileRef?
    private var fileRate: Double = 0
    private var mono = false
    private(set) var duration: Double = 0
    private var readError: Error?

    init(_ bytes: Bytes, format: AVAudioFormat) throws {
        self.bytes = bytes
        let me = Unmanaged.passUnretained(self).toOpaque()
        var st = AudioFileOpenWithCallbacks(
            me,
            { client, offset, count, buffer, actual in
                let d = Unmanaged<AudioFileDecoder>.fromOpaque(client).takeUnretainedValue()
                do {
                    actual.pointee = UInt32(try d.bytes.read(offset, Int(count), into: buffer))
                    return noErr
                } catch {
                    d.readError = error
                    actual.pointee = 0
                    return kAudioFileUnspecifiedError
                }
            },
            nil,
            { client in
                Unmanaged<AudioFileDecoder>.fromOpaque(client).takeUnretainedValue().bytes.size
            },
            nil, 0, &file)
        guard st == noErr, let file else { throw readError ?? DecodeError("AudioFileOpen", st) }
        st = ExtAudioFileWrapAudioFileID(file, false, &ext)
        guard st == noErr, let ext else { throw DecodeError("ExtAudioFileWrap", st) }

        var asbd = AudioStreamBasicDescription()
        var size = UInt32(MemoryLayout.size(ofValue: asbd))
        ExtAudioFileGetProperty(ext, kExtAudioFileProperty_FileDataFormat, &size, &asbd)
        fileRate = asbd.mSampleRate

        var client = format.streamDescription.pointee
        if asbd.mChannelsPerFrame == 1 { client.mChannelsPerFrame = 1 }  // upmixed in read()
        st = ExtAudioFileSetProperty(
            ext, kExtAudioFileProperty_ClientDataFormat,
            UInt32(MemoryLayout.size(ofValue: client)), &client)
        guard st == noErr else { throw DecodeError("set client format", st) }
        mono = asbd.mChannelsPerFrame == 1

        var frames: Int64 = 0
        size = UInt32(MemoryLayout.size(ofValue: frames))
        ExtAudioFileGetProperty(ext, kExtAudioFileProperty_FileLengthFrames, &size, &frames)
        if fileRate > 0 { duration = Double(frames) / fileRate }
    }

    deinit {
        if let ext { ExtAudioFileDispose(ext) }  // the wrapped AudioFileID stays ours
        if let file { AudioFileClose(file) }
    }

    func read(into buffer: AVAudioPCMBuffer) throws {
        var frames = buffer.frameCapacity
        // The list advertises the buffer's current length; offer its capacity.
        let list = UnsafeMutableAudioBufferListPointer(buffer.mutableAudioBufferList)
        for i in 0..<list.count { list[i].mDataByteSize = frames * 4 }
        if mono { list.unsafeMutablePointer.pointee.mNumberBuffers = 1 }
        let st = ExtAudioFileRead(ext!, &frames, list.unsafeMutablePointer)
        if mono { list.unsafeMutablePointer.pointee.mNumberBuffers = 2 }
        guard st == noErr else { throw readError ?? DecodeError("ExtAudioFileRead", st) }
        buffer.frameLength = frames
        if mono, frames > 0, let ch = buffer.floatChannelData {
            ch[1].update(from: ch[0], count: Int(frames))
        }
    }

    func seek(to seconds: Double) throws {
        let st = ExtAudioFileSeek(ext!, Int64(seconds * fileRate))
        guard st == noErr else { throw readError ?? DecodeError("ExtAudioFileSeek", st) }
    }
}
