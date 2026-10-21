import Foundation

/// Fills a `SparseFile` from the server's stream endpoint with Range requests.
///
/// It reads on from where it is, and jumps when a reader waits somewhere it
/// will not reach soon: before it, or more than 1 MB past it, which the phone
/// treats the same way (plan 019). That covers a seek past what has arrived,
/// and an MP4 whose index is at the end. A failure is retried, backing off,
/// for as long as the fetch lives: the player waits for the song.
///
/// With `ahead` set, the file is a long file's window (plan 037): the fetch
/// stops `ahead` bytes past the reader, goes on as the reader moves, and
/// rolls the file once more than twice `behind` sits before the reader.
public final class Fetch: NSObject, URLSessionDataDelegate, @unchecked Sendable {
    public let file: SparseFile
    private let request: URLRequest
    private let ahead: Int64
    private let behind: Int64
    private let lock = NSLock()
    private var session: URLSession!
    private var task: URLSessionDataTask?
    private var head: Int64 = 0  // where the current request writes next
    /// Where the reader is, which a window is fetched ahead of and kept
    /// behind; -1 until it is known. Set only once the reader has read on in
    /// one piece for `anchor` bytes: a decoder finding its place reads a page
    /// here and a page there, and a window at each probe would fetch the
    /// whole file in pieces.
    private var readAt: Int64 = -1
    private var lastEnd: Int64 = -1  // where the reader's last read ended
    private var runStart: Int64 = -1  // where its run of reads in one piece began
    private var failures = 0
    private var cancelled = false
    /// The file is whole.
    public var completed: (@Sendable () -> Void)?
    /// The server no longer has the song (404, 410): final.
    public var gone: (@Sendable () -> Void)?
    /// Each response: whether the server could be reached.
    public var reachable: (@Sendable (Bool) -> Void)?
    /// A window has all it wants for now: the cache may fetch something else.
    public var paused: (@Sendable () -> Void)?

    /// A request is under way.
    public var active: Bool { lock.withLock { task != nil } }

    /// A jump to a waiting reader fetches at least this much, whatever the window says.
    static let minFetch: Int64 = 256 << 10
    /// A paused window is fetched on once the reader has used this much of it.
    static let refill: Int64 = 1 << 20
    /// A reader that has read on in one piece for this much has settled: the window anchors there.
    static let anchor: Int64 = 256 << 10

    /// `request` carries the URL and the token; the range is added here.
    /// `ahead` and `behind` in bytes make the file a window; 0 fetches it whole.
    public init(request: URLRequest, file: SparseFile, ahead: Int64 = 0, behind: Int64 = 0) {
        self.request = request
        self.file = file
        self.ahead = ahead
        self.behind = behind
        super.init()
        let c = URLSessionConfiguration.ephemeral
        // A stall is given up after 10 s and tried again; the next try may well work (plan 019).
        c.timeoutIntervalForRequest = 10
        session = URLSession(configuration: c, delegate: self, delegateQueue: nil)
        file.wanted = { [weak self] at in self?.wanted(at) }
        if ahead > 0 { file.reading = { [weak self] at, count in self?.reading(at, count) } }
    }

    /// Where the window starts before any reader has settled: the start, for the next file's prefetch.
    public func anchor(_ at: Int64) {
        lock.withLock {
            guard readAt < 0 else { return }
            readAt = at
            if task == nil, let n = next() { request(from: n) }
        }
    }

    public func start() {
        lock.withLock { if let n = next() { request(from: n) } }
    }

    public func cancel() {
        lock.withLock {
            cancelled = true
            task?.cancel()
            task = nil
        }
        session.invalidateAndCancel()
        file.fail(BytesError.cancelled)
    }

    private func wanted(_ at: Int64) {
        lock.withLock {
            if !cancelled && (task == nil || at < head || at > head + 1 << 20) { request(from: at) }
        }
    }

    /// The reader reads `count` at `at`: anchors the window once the reader
    /// has settled, and fetches on a window that has run down by `refill`.
    private func reading(_ at: Int64, _ count: Int64) {
        lock.withLock {
            if at == lastEnd {
                if at - runStart >= Self.anchor { readAt = at }
            } else {
                runStart = at
            }
            lastEnd = at + count
            guard !cancelled, task == nil, readAt >= 0,
                file.arrived(from: readAt) < min(file.size, readAt + ahead - Self.refill), let n = next()
            else { return }
            request(from: n)
        }
    }

    /// Where to fetch next, or nil when nothing is wanted now: the file is
    /// whole, or the window is full until the reader moves. Call under `lock`.
    private func next() -> Int64? {
        if file.complete { return nil }
        if ahead == 0 {
            let gap = file.arrived(from: 0)
            return gap < file.size ? gap : nil
        }
        guard readAt >= 0 else { return nil }
        let a = file.arrived(from: readAt)
        return a < file.size && a < readAt + ahead ? a : nil
    }

    /// Where a request from `offset` ends (exclusive): the next run already
    /// here, and for a window its end when `offset` is inside it, else
    /// `minFetch`: a jump to a reader waiting elsewhere fetches enough for
    /// that read and no more. Call under `lock`.
    private func goal(from offset: Int64) -> Int64 {
        var g = file.size
        if ahead > 0 {
            let inside = readAt >= 0 && offset >= readAt && offset < readAt + ahead
            g = min(g, inside ? readAt + ahead : offset + Self.minFetch)
        }
        if let n = file.nextStart(after: offset) { g = min(g, n) }
        return g
    }

    /// Call under `lock`.
    private func request(from offset: Int64) {
        task?.cancel()
        guard offset < file.size, !cancelled else {
            task = nil
            return
        }
        var req = request
        // A song is asked for to its end; a window only as far as its goal.
        let range = ahead > 0 ? "bytes=\(offset)-\(goal(from: offset) - 1)" : "bytes=\(offset)-"
        req.setValue(range, forHTTPHeaderField: "Range")
        head = offset
        let t = session.dataTask(with: req)
        task = t
        t.resume()
    }

    public func urlSession(
        _ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse,
        completionHandler: @escaping (URLSession.ResponseDisposition) -> Void
    ) {
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        reachable?(true)
        if status == 404 || status == 410 {
            file.fail(BytesError.failed("The song is no longer on the server"))
            gone?()
            completionHandler(.cancel)
            return
        }
        guard status == 206 || (status == 200 && lock.withLock({ head }) == 0) else {
            completionHandler(.cancel)
            return
        }
        file.setFailing(false)
        completionHandler(.allow)
    }

    public func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        let (at, roll): (Int64?, Int64?) = lock.withLock {
            guard dataTask == task else { return (nil, nil) }
            defer { head += Int64(data.count) }
            // More than twice `behind` sits before the reader: roll before writing on.
            let keepFrom = readAt - behind
            let due = ahead > 0 && readAt >= 0 && (file.firstStart ?? readAt) < readAt - 2 * behind
            return (head, due ? keepFrom : nil)
        }
        guard let at else { return }
        if let roll { file.drop(before: roll) }
        file.write(at, data)
    }

    public func urlSession(
        _ session: URLSession, task done: URLSessionTask, didCompleteWithError error: Error?
    ) {
        let whole: Bool = lock.withLock {
            guard done == task, !cancelled else { return false }
            task = nil
            if file.complete { return true }
            if let error, (error as NSError).code == NSURLErrorCancelled { return false }
            let status = (done.response as? HTTPURLResponse)?.statusCode ?? 0
            if error != nil || status >= 500 || status == 408 || status == 429 {
                if error != nil { reachable?(false) }
                failures += 1
                file.setFailing(true)
                // Retry from the first gap, backing off from 1 s to 5 s, as the phone's cache does.
                let delay = min(5.0, Double(failures))
                DispatchQueue.global().asyncAfter(deadline: .now() + delay) { [weak self] in
                    guard let self else { return }
                    self.lock.withLock {
                        if self.task == nil, let n = self.next() { self.request(from: n) }
                    }
                }
                return false
            }
            // Finished a range: fill whatever gap is left, or pause at the window's end.
            failures = 0
            if let n = next() {
                request(from: n)
            } else if ahead > 0 {
                DispatchQueue.global().async { [weak self] in self?.paused?() }
            }
            return false
        }
        if whole { completed?() }
    }
}
