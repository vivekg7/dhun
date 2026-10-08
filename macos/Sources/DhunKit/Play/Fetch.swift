import Foundation

/// Fills a `SparseFile` from the server's stream endpoint with Range requests.
///
/// It reads on from where it is, and jumps when a reader waits somewhere it
/// will not reach soon: before it, or more than 1 MB past it, which the phone
/// treats the same way (plan 019). That covers a seek past what has arrived,
/// and an MP4 whose index is at the end. A failure is retried, backing off,
/// for as long as the fetch lives: the player waits for the song.
public final class Fetch: NSObject, URLSessionDataDelegate, @unchecked Sendable {
    public let file: SparseFile
    private let request: URLRequest
    private let lock = NSLock()
    private var session: URLSession!
    private var task: URLSessionDataTask?
    private var head: Int64 = 0  // where the current request writes next
    private var failures = 0
    private var cancelled = false
    /// The file is whole.
    public var completed: (@Sendable () -> Void)?
    /// The server no longer has the song (404, 410): final.
    public var gone: (@Sendable () -> Void)?
    /// Each response: whether the server could be reached.
    public var reachable: (@Sendable (Bool) -> Void)?

    /// `request` carries the URL and the token; the range is added here.
    public init(request: URLRequest, file: SparseFile) {
        self.request = request
        self.file = file
        super.init()
        let c = URLSessionConfiguration.ephemeral
        // A stall is given up after 10 s and tried again; the next try may well work (plan 019).
        c.timeoutIntervalForRequest = 10
        session = URLSession(configuration: c, delegate: self, delegateQueue: nil)
        file.wanted = { [weak self] at in self?.wanted(at) }
    }

    public func start() { lock.withLock { request(from: file.arrived(from: 0)) } }

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

    /// Call under `lock`.
    private func request(from offset: Int64) {
        task?.cancel()
        guard offset < file.size, !cancelled else {
            task = nil
            return
        }
        var req = request
        req.setValue("bytes=\(offset)-", forHTTPHeaderField: "Range")
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
        let at: Int64? = lock.withLock {
            guard dataTask == task else { return nil }
            defer { head += Int64(data.count) }
            return head
        }
        if let at { file.write(at, data) }
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
                        if self.task == nil { self.request(from: self.file.arrived(from: 0)) }
                    }
                }
                return false
            }
            // Finished a range: fill whatever gap is left.
            failures = 0
            let gap = file.arrived(from: 0)
            if gap < file.size { request(from: gap) }
            return false
        }
        if whole { completed?() }
    }
}
