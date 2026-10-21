import Foundation

/// Random access to one song's bytes, some of which may still be arriving.
///
/// The decoders read through this, so one code path plays a downloaded
/// file, a cached file, a cache file still being written and the stream
/// (plan 024). Reads run on the decoding thread and may block; they never
/// run on the audio render thread.
public protocol Bytes: AnyObject, Sendable {
    /// The whole file's size, known before its bytes are (from the library
    /// or the response's Content-Length).
    var size: Int64 { get }
    /// Copies up to `count` bytes at `offset`, waiting for them if they have
    /// not arrived. Returns fewer only at the end of the file.
    func read(_ offset: Int64, _ count: Int, into: UnsafeMutableRawPointer) throws -> Int
    /// Makes a read that is waiting for bytes throw `cancelled` (a skip or a
    /// seek); later reads wait as before.
    func interrupt()
    /// Where the bytes run out, reading on from `offset`.
    func arrived(from offset: Int64) -> Int64
}

extension Bytes {
    public func interrupt() {}
    public func arrived(from offset: Int64) -> Int64 { size }
}

public enum BytesError: Error {
    case cancelled
    case failed(String)
}

/// A complete file on disk: a download, or a finished cache entry.
public final class FileBytes: Bytes {
    private let fd: Int32
    public let size: Int64

    public init(_ url: URL) throws {
        fd = open(url.path, O_RDONLY)
        guard fd >= 0 else { throw BytesError.failed("open \(url.path): \(errno)") }
        var st = stat()
        fstat(fd, &st)
        size = st.st_size
    }

    deinit { close(fd) }

    public func read(_ offset: Int64, _ count: Int, into: UnsafeMutableRawPointer) throws -> Int {
        let n = pread(fd, into, count, off_t(offset))
        guard n >= 0 else { throw BytesError.failed("read: \(errno)") }
        return n
    }
}

/// A file being filled out of order: the cache entry of a song that plays
/// while it arrives. Whoever fetches calls `write`; readers wait for their
/// range. `wanted` tells the fetcher where a reader is stuck, so it can jump
/// there (a seek past what has arrived, or an MP4 whose index is at the end);
/// `reading` tells it where every read is, so a window can follow the reader
/// (plan 037).
public final class SparseFile: Bytes, @unchecked Sendable {
    public let size: Int64
    public let url: URL
    private var fd: Int32
    /// The file before the last `drop`: closed at the next one, when no read can still be on it.
    private var retired: Int32 = -1
    private let cond = NSCondition()
    private var have: [Range<Int64>] = []  // sorted, disjoint
    private var failure: Error?
    private var interrupts = 0
    /// Called (off the lock) with the offset a reader waits for.
    public var wanted: (@Sendable (Int64) -> Void)?
    /// Called (off the lock) with the offset and length of every read.
    public var reading: (@Sendable (Int64, Int64) -> Void)?

    public init(_ url: URL, size: Int64) throws {
        self.url = url
        self.size = size
        fd = open(url.path, O_RDWR | O_CREAT, 0o644)
        guard fd >= 0 else { throw BytesError.failed("open \(url.path): \(errno)") }
        ftruncate(fd, off_t(size))
    }

    deinit {
        close(fd)
        if retired >= 0 { close(retired) }
    }

    public var complete: Bool {
        cond.lock()
        defer { cond.unlock() }
        return have.count == 1 && have[0] == 0..<size
    }

    public func arrived(from offset: Int64) -> Int64 {
        cond.lock()
        defer { cond.unlock() }
        return end(from: offset)
    }

    /// Where the first bytes held start, or nil while none has arrived.
    public var firstStart: Int64? {
        cond.lock()
        defer { cond.unlock() }
        return have.first?.lowerBound
    }

    /// Where the next run after `offset` starts, or nil: a fetch from `offset` stops there.
    public func nextStart(after offset: Int64) -> Int64? {
        cond.lock()
        defer { cond.unlock() }
        return have.first { $0.lowerBound > offset }?.lowerBound
    }

    public func write(_ offset: Int64, _ data: Data) {
        cond.lock()
        let fd = fd
        cond.unlock()
        data.withUnsafeBytes { _ = pwrite(fd, $0.baseAddress, data.count, off_t(offset)) }
        cond.lock()
        add(offset..<offset + Int64(data.count))
        cond.broadcast()
        cond.unlock()
    }

    /// Copies the bytes from `keepFrom` on into a fresh file that takes this
    /// one's place, and forgets the rest: a window rolls behind its reader
    /// (plan 037). Called by the writer between writes, so nothing is lost;
    /// a read on the old file goes on, as it stays open until the next drop.
    public func drop(before keepFrom: Int64) {
        cond.lock()
        let runs = have.compactMap { r -> Range<Int64>? in
            r.upperBound > keepFrom ? max(r.lowerBound, keepFrom)..<r.upperBound : nil
        }
        let old = fd
        cond.unlock()
        let tmp = url.appendingPathExtension("tmp")
        let nfd = open(tmp.path, O_RDWR | O_CREAT | O_TRUNC, 0o644)
        guard nfd >= 0 else { return }
        ftruncate(nfd, off_t(size))
        var buf = [UInt8](repeating: 0, count: 256 * 1024)
        for r in runs {
            var p = r.lowerBound
            while p < r.upperBound {
                let n = pread(old, &buf, min(buf.count, Int(r.upperBound - p)), off_t(p))
                guard n > 0 else { break }
                pwrite(nfd, buf, n, off_t(p))
                p += Int64(n)
            }
        }
        guard rename(tmp.path, url.path) == 0 else {
            close(nfd)
            unlink(tmp.path)
            return
        }
        cond.lock()
        if retired >= 0 { close(retired) }
        retired = fd
        fd = nfd
        have = runs
        cond.unlock()
    }

    public func interrupt() {
        cond.lock()
        interrupts += 1
        cond.broadcast()
        cond.unlock()
    }

    /// The fetch failed and is waiting to try again: the player says it is waiting.
    public private(set) var failing = false
    public func setFailing(_ f: Bool) {
        cond.lock()
        failing = f
        cond.unlock()
    }

    public func fail(_ error: Error) {
        cond.lock()
        failure = error
        cond.broadcast()
        cond.unlock()
    }

    public func read(_ offset: Int64, _ count: Int, into: UnsafeMutableRawPointer) throws -> Int {
        let want = min(Int64(count), size - offset)
        guard want > 0 else { return 0 }
        reading?(offset, want)
        cond.lock()
        var told = false
        let asOf = interrupts
        while end(from: offset) < offset + want {
            if let failure {
                cond.unlock()
                throw failure
            }
            if interrupts != asOf {
                cond.unlock()
                throw BytesError.cancelled
            }
            if !told {
                told = true
                let at = end(from: offset)
                cond.unlock()
                wanted?(at)
                cond.lock()
                continue
            }
            cond.wait()
        }
        // The file as of now: a drop meanwhile moved these bytes to a new one.
        let fd = fd
        cond.unlock()
        let n = pread(fd, into, Int(want), off_t(offset))
        guard n >= 0 else { throw BytesError.failed("read: \(errno)") }
        return n
    }

    private func end(from offset: Int64) -> Int64 {
        for r in have where r.contains(offset) { return r.upperBound }
        return offset
    }

    private func add(_ r: Range<Int64>) {
        var merged = r
        have.removeAll { other in
            guard
                other.overlaps(merged) || other.upperBound == merged.lowerBound
                    || merged.upperBound == other.lowerBound
            else { return false }
            merged = min(other.lowerBound, merged.lowerBound)..<max(other.upperBound, merged.upperBound)
            return true
        }
        have.append(merged)
        have.sort { $0.lowerBound < $1.lowerBound }
    }
}
