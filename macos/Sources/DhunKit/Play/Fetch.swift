import Foundation

/// Fills a `SparseFile` from the server's stream endpoint with Range requests.
///
/// It reads on from where it is, and jumps when a reader waits somewhere it
/// will not reach soon: before it, or more than 1 MB past it, which the phone
/// treats the same way (plan 019). That covers a seek past what has arrived,
/// and an MP4 whose index is at the end.
public final class Fetch: NSObject, URLSessionDataDelegate, @unchecked Sendable {
    public let file: SparseFile
    private let url: URL
    private let token: String
    private let lock = NSLock()
    private var session: URLSession!
    private var task: URLSessionDataTask?
    private var head: Int64 = 0  // where the current request writes next
    private var failures = 0

    public init(url: URL, token: String, file: SparseFile) {
        self.url = url
        self.token = token
        self.file = file
        super.init()
        session = URLSession(configuration: .ephemeral, delegate: self, delegateQueue: nil)
        file.wanted = { [weak self] at in self?.wanted(at) }
    }

    public func start() { lock.withLock { request(from: file.arrived(from: 0)) } }

    public func cancel() {
        lock.withLock {
            task?.cancel()
            task = nil
        }
        session.invalidateAndCancel()
        file.fail(BytesError.cancelled)
    }

    private func wanted(_ at: Int64) {
        lock.withLock {
            if task == nil || at < head || at > head + 1 << 20 { request(from: at) }
        }
    }

    /// Call under `lock`.
    private func request(from offset: Int64) {
        task?.cancel()
        guard offset < file.size else {
            task = nil
            return
        }
        var req = URLRequest(url: url)
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
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
        guard status == 206 || (status == 200 && lock.withLock({ head }) == 0) else {
            file.fail(BytesError.failed("stream: HTTP \(status)"))
            completionHandler(.cancel)
            return
        }
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
        lock.withLock {
            guard done == task else { return }
            task = nil
            if let error, (error as NSError).code != NSURLErrorCancelled {
                failures += 1
                // Retry from the first gap, backing off, as the phone's cache does.
                let delay = min(5.0, Double(failures))
                DispatchQueue.global().asyncAfter(deadline: .now() + delay) { [weak self] in
                    guard let self else { return }
                    self.lock.withLock {
                        if self.task == nil { self.request(from: self.file.arrived(from: 0)) }
                    }
                }
                return
            }
            // Finished a range: fill whatever gap is left.
            failures = 0
            let gap = file.arrived(from: 0)
            if gap < file.size { request(from: gap) }
        }
    }
}
