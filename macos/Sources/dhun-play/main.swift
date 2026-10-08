// The playback spike's harness (plan 024, step 0).
//
//   dhun-play check FILE...          decode each file: length, time, silence at the joins
//   dhun-play play [opts] FILE...    play the files back to back; commands on stdin:
//                                    s SECONDS (seek), r SPEED, p SEMITONES, n (next), q
//     --grow KBPS                    serve each file through a SparseFile filled at KBPS,
//                                    jumping to wherever a reader waits (a Range request)
//     --from SECONDS                 start the first file there
//     --server URL --token T         FILEs are song IDs, streamed from a Dhun server
import AVFoundation
import DhunKit
import Foundation

let args = Array(CommandLine.arguments.dropFirst())
guard let mode = args.first else {
    print("usage: dhun-play check|play [--grow KBPS] [--from S] FILE...")
    exit(2)
}
var files: [String] = []
var grow: Double?
var from = 0.0
var server: URL?
var token = ""
var sizes: [String: Int64] = [:]
var fetches: [Fetch] = []
var i = 1
while i < args.count {
    switch args[i] {
    case "--grow":
        grow = Double(args[i + 1])
        i += 2
    case "--server":
        server = URL(string: args[i + 1])
        i += 2
    case "--token":
        token = args[i + 1]
        i += 2
    case "--from":
        from = Double(args[i + 1]) ?? 0
        i += 2
    default:
        files.append(args[i])
        i += 1
    }
}

/// Where --grow and --server keep their partial files; removed on exit.
let scratch = FileManager.default.temporaryDirectory.appendingPathComponent("dhun-play-\(getpid())")
try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
atexit { try? FileManager.default.removeItem(at: scratch) }

/// A file arriving at `kbps`, in order, from wherever a reader last waited.
final class Arrival: @unchecked Sendable {
    let sparse: SparseFile
    private let source: FileHandle
    private let lock = NSLock()
    private var head: Int64 = 0
    private(set) var jumps = 0

    init(_ path: String, kbps: Double) throws {
        source = try FileHandle(forReadingFrom: URL(fileURLWithPath: path))
        let size = try FileManager.default.attributesOfItem(atPath: path)[.size] as! Int64
        let tmp = scratch.appendingPathComponent(UUID().uuidString + ".part")
        sparse = try SparseFile(tmp, size: size)
        sparse.wanted = { [weak self] at in
            guard let self else { return }
            // A reader waiting more than 1 MB past the fetch: jump, as the
            // app's fetcher will with a new Range request.
            self.lock.withLock {
                if at < self.head || at > self.head + 1 << 20 {
                    self.head = at
                    self.jumps += 1
                }
            }
        }
        let chunk = 16 * 1024
        let sleep = Double(chunk) / (kbps * 1024)
        Thread.detachNewThread { [self] in
            while !sparse.complete {
                let at = lock.withLock { head }
                let start = sparse.arrived(from: at)
                guard start < sparse.size else {
                    lock.withLock { head = 0 }
                    continue
                }
                try? source.seek(toOffset: UInt64(start))
                let data = source.readData(ofLength: chunk)
                sparse.write(start, data)
                lock.withLock { if head == at { head = start + Int64(data.count) } }
                Thread.sleep(forTimeInterval: sleep)
            }
        }
    }
}

if let server {
    var req = URLRequest(url: server.appendingPathComponent("api/v1/library"))
    req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    let sem = DispatchSemaphore(value: 0)
    URLSession.shared.dataTask(with: req) { data, _, _ in
        let lib = try! JSONSerialization.jsonObject(with: data!) as! [String: Any]
        for song in lib["songs"] as! [[String: Any]] {
            sizes["\(song["id"]!)"] = (song["size"] as! NSNumber).int64Value
        }
        sem.signal()
    }.resume()
    sem.wait()
}

func bytes(_ path: String) throws -> Bytes {
    if let server, let size = sizes[path] {
        let tmp = scratch.appendingPathComponent(UUID().uuidString + ".part")
        let f = Fetch(
            url: server.appendingPathComponent("api/v1/stream/\(path)"), token: token,
            file: try SparseFile(tmp, size: size))
        fetches.append(f)
        f.start()
        return f.file
    }
    if let grow { return try Arrival(path, kbps: grow).sparse }
    return try FileBytes(URL(fileURLWithPath: path))
}

func name(_ i: Int) -> String { URL(fileURLWithPath: files[i]).lastPathComponent }

switch mode {
case "check":
    let format = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 2)!
    var lastTail: Int?
    var lastSample: Float?
    for (n, path) in files.enumerated() {
        do {
            let t0 = Date()
            let d = try openDecoder(try bytes(path), format: format)
            let opened = Date().timeIntervalSince(t0)
            let buf = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 8192)!
            var frames = 0
            var lead = 0
            var tail = 0
            var heardSound = false
            var peak: Float = 0
            var first: Float?
            var final: Float = 0
            var step: Float = 0  // mean sample-to-sample change, to judge a join by
            var prev: Float = 0
            while true {
                try d.read(into: buf)
                if buf.frameLength == 0 { break }
                let l = buf.floatChannelData![0]
                for k in 0..<Int(buf.frameLength) {
                    if first == nil { first = l[k] }
                    step += abs(l[k] - prev)
                    prev = l[k]
                    let a = abs(l[k])
                    peak = max(peak, a)
                    if a < 1e-4 {
                        tail += 1
                        if !heardSound { lead += 1 }
                    } else {
                        heardSound = true
                        tail = 0
                    }
                }
                frames += Int(buf.frameLength)
                final = l[Int(buf.frameLength) - 1]
            }
            let got = Double(frames) / format.sampleRate
            print(
                String(
                    format:
                        "%@\n  open %.3fs, decode %.2fs, declared %.3fs, decoded %.3fs (diff %+.1f ms), peak %.2f",
                    name(n), opened, Date().timeIntervalSince(t0), d.duration, got,
                    (got - d.duration) * 1000, peak))
            print(
                String(
                    format: "  silence: %.1f ms at the start, %.1f ms at the end",
                    Double(lead) / 48, Double(tail) / 48))
            if let lastTail {
                print(
                    String(format: "  join with previous: %.1f ms of silence", Double(lastTail + lead) / 48))
            }
            if let lastSample, let first {
                print(
                    String(
                        format: "  join step %.4f (this song's mean step %.4f)",
                        abs(first - lastSample), step / Float(max(frames, 1))))
            }
            lastTail = tail
            lastSample = final
        } catch {
            print("\(name(n))\n  FAILED: \(error)")
            lastTail = nil
        }
    }

case "play":
    let engine = Engine()
    var nextIndex = 1
    let lock = NSLock()
    engine.next = {
        lock.withLock {
            while nextIndex < files.count {
                let n = nextIndex
                nextIndex += 1
                if let d = try? openDecoder(try bytes(files[n]), format: engine.format) { return (n, d) }
                print("cannot open \(name(n))")
            }
            return nil
        }
    }
    engine.failed = { song, error in print("\(name(song)): \(error)") }
    let t0 = Date()
    try engine.play(song: 0, decoder: try openDecoder(try bytes(files[0]), format: engine.format), from: from)
    print(
        String(format: "started in %.3fs at %.0f Hz", Date().timeIntervalSince(t0), engine.format.sampleRate))

    var shown = -1
    Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { _ in
        guard let p = engine.position() else {
            print("end")
            exit(0)
        }
        if p.song != shown {
            shown = p.song
            print("▶ \(name(p.song))")
        }
        print(String(format: "  %6.1fs  speed %.2f  pitch %+.0f", p.seconds, engine.speed, engine.semitones))
    }
    Thread.detachNewThread {
        while let line = readLine() {
            let parts = line.split(separator: " ")
            guard let cmd = parts.first else { continue }
            let value = parts.count > 1 ? Double(parts[1]) ?? 0 : 0
            do {
                switch cmd {
                case "s": try engine.seek(to: value)
                case "r": engine.speed = Float(value)
                case "p": engine.semitones = Float(value)
                case "n":
                    if let (n, d) = engine.next?() { try engine.play(song: n, decoder: d) }
                case "q": exit(0)
                default: print("?")
                }
            } catch { print("error: \(error)") }
        }
    }
    RunLoop.main.run()

default:
    print("unknown mode \(mode)")
    exit(2)
}
