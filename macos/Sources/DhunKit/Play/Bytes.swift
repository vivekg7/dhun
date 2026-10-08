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
/// there (a seek past what has arrived, or an MP4 whose index is at the end).
public final class SparseFile: Bytes, @unchecked Sendable {
    public let size: Int64
    public let url: URL
    private let fd: Int32
    private let cond = NSCondition()
    private var have: [Range<Int64>] = []  // sorted, disjoint
    private var failure: Error?
    /// Called (off the lock) with the offset a reader waits for.
    public var wanted: (@Sendable (Int64) -> Void)?

    public init(_ url: URL, size: Int64) throws {
        self.url = url
        self.size = size
        fd = open(url.path, O_RDWR | O_CREAT, 0o644)
        guard fd >= 0 else { throw BytesError.failed("open \(url.path): \(errno)") }
        ftruncate(fd, off_t(size))
    }

    deinit { close(fd) }

    public var complete: Bool {
        cond.lock()
        defer { cond.unlock() }
        return have.count == 1 && have[0] == 0..<size
    }

    /// Where the bytes run out, reading on from `offset`.
    public func arrived(from offset: Int64) -> Int64 {
        cond.lock()
        defer { cond.unlock() }
        return end(from: offset)
    }

    public func write(_ offset: Int64, _ data: Data) {
        data.withUnsafeBytes { _ = pwrite(fd, $0.baseAddress, data.count, off_t(offset)) }
        cond.lock()
        add(offset..<offset + Int64(data.count))
        cond.broadcast()
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
        cond.lock()
        var told = false
        while end(from: offset) < offset + want {
            if let failure {
                cond.unlock()
                throw failure
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
