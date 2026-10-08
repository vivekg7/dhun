import Foundation

/// This Mac's own settings and cursors, the phone's `Prefs`.
public final class Prefs: @unchecked Sendable {
    private let d: UserDefaults
    private let tokenFile: URL
    private let lock = NSLock()
    private var tokenCache: String?

    /// `tokenFile` is in the app's own folder.
    public init(defaults: UserDefaults = .standard, tokenFile: URL) {
        d = defaults
        self.tokenFile = tokenFile
    }

    // Account and cursors.
    public var server: String {
        get { str("server") }
        set { d.set(newValue, forKey: "server") }
    }
    public var user: String {
        get { str("user") }
        set { d.set(newValue, forKey: "user") }
    }
    public var admin: Bool {
        get { d.bool(forKey: "admin") }
        set { d.set(newValue, forKey: "admin") }
    }
    public var adminKnown: Bool { d.object(forKey: "admin") != nil }
    public var serverVersion: String {
        get { str("serverVersion") }
        set { d.set(newValue, forKey: "serverVersion") }
    }
    public var libraryVersion: Int {
        get { d.integer(forKey: "libraryVersion") }
        set { d.set(newValue, forKey: "libraryVersion") }
    }
    public var libraryFormat: Int {
        get { d.integer(forKey: "libraryFormat") }
        set { d.set(newValue, forKey: "libraryFormat") }
    }
    public var syncVersion: Int {
        get { d.integer(forKey: "syncVersion") }
        set { d.set(newValue, forKey: "syncVersion") }
    }
    public var deviceId: Int {
        get { d.integer(forKey: "deviceId") }
        set { d.set(newValue, forKey: "deviceId") }
    }
    /// When this Mac last reported its playback, and the hand-off last dismissed (plan 017).
    public var stateAt: Int {
        get { d.integer(forKey: "stateAt") }
        set { d.set(newValue, forKey: "stateAt") }
    }
    public var handoffDismissed: String {
        get { str("handoffDismissed") }
        set { d.set(newValue, forKey: "handoffDismissed") }
    }
    public var activeQueue: String {
        get { str("activeQueue") }
        set { d.set(newValue, forKey: "activeQueue") }
    }

    // Appearance.
    public var palette: Int {
        get { d.integer(forKey: "palette") }
        set { d.set(newValue, forKey: "palette") }
    }
    /// system, light or dark.
    public var theme: String {
        get { str("theme", "system") }
        set { d.set(newValue, forKey: "theme") }
    }

    // Storage and network (plans 012, 019): the phone's defaults.
    public var downloadLimitGb: Int {
        get { int("downloadLimitGb", 10) }
        set { d.set(newValue, forKey: "downloadLimitGb") }
    }
    public var cacheLimitGb: Int {
        get { int("cacheLimitGb", 3) }
        set { d.set(newValue, forKey: "cacheLimitGb") }
    }
    /// Downloads wait for a network that is not "expensive" (a phone's hotspot).
    public var downloadOnExpensive: Bool {
        get { d.bool(forKey: "downloadOnExpensive") }
        set { d.set(newValue, forKey: "downloadOnExpensive") }
    }

    // Playback.
    public var speed: Double {
        get { double("speed", 1) }
        set { d.set(newValue, forKey: "speed") }
    }
    public var semitones: Int {
        get { d.integer(forKey: "semitones") }
        set { d.set(newValue, forKey: "semitones") }
    }
    public var openListen: String {
        get { str("openListen") }
        set { d.set(newValue, forKey: "openListen") }
    }
    public func queueSource(_ queue: String) -> String { str("source.\(queue)") }
    public func setQueueSource(_ queue: String, _ source: String) { d.set(source, forKey: "source.\(queue)") }

    /// Forgets the account's cursors and state; the server address stays.
    public func forgetAccount() {
        for k in [
            "user", "admin", "serverVersion", "libraryVersion", "libraryFormat", "syncVersion", "deviceId",
            "stateAt",
            "handoffDismissed", "activeQueue", "openListen",
        ] { d.removeObject(forKey: k) }
        for k in d.dictionaryRepresentation().keys where k.hasPrefix("source.") { d.removeObject(forKey: k) }
    }

    private func str(_ k: String, _ def: String = "") -> String { d.string(forKey: k) ?? def }
    private func int(_ k: String, _ def: Int) -> Int {
        d.object(forKey: k) == nil ? def : d.integer(forKey: k)
    }
    private func double(_ k: String, _ def: Double) -> Double {
        d.object(forKey: k) == nil ? def : d.double(forKey: k)
    }

    // MARK: Token

    /// The sign-in token, in a file only this user can read, as the phone
    /// keeps it in its app-private storage. Not the Keychain: its access is
    /// tied to the code signature, which an ad-hoc signed build changes every
    /// time, so each update would ask for the login password (plan 024).
    public var token: String {
        get {
            lock.withLock {
                if let tokenCache { return tokenCache }
                let t = (try? String(contentsOf: tokenFile, encoding: .utf8)) ?? ""
                tokenCache = t
                return t
            }
        }
        set {
            lock.withLock {
                tokenCache = newValue
                let fm = FileManager.default
                if newValue.isEmpty {
                    try? fm.removeItem(at: tokenFile)
                    return
                }
                fm.createFile(
                    atPath: tokenFile.path, contents: Data(newValue.utf8),
                    attributes: [.posixPermissions: 0o600])
            }
        }
    }
}

/// "nas" → "http://nas:8585", as the phone reads what was typed. Only plain
/// HTTP gets the default port: HTTPS is `tailscale serve`, on 443.
public func serverURL(_ typed: String) -> String {
    var s = typed.trimmingCharacters(in: .whitespaces)
    while s.hasSuffix("/") { s.removeLast() }
    if !s.contains("://") { s = "http://" + s }
    let rest = s.components(separatedBy: "://")[1]
    let host = rest.split(separator: "/", maxSplits: 1).first.map(String.init) ?? ""
    if !host.contains(":") && s.hasPrefix("http://") {
        s = "http://" + host + ":8585" + rest.dropFirst(host.count)
    }
    return s
}
