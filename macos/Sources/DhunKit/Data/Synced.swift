/// The settings that follow the user to every device: each one's name in the
/// store, the value used while it is unset, and the values Settings offers
/// (docs/plans/034_settings_text_defaults_reset.md). The phone and the web
/// keep the same table; make check fails when a default differs.
public enum Synced {
    public static let previousRestart = Key("previous.restart", true)
    public static let minMinutes = Key("longFiles.minMinutes", 15, [5, 10, 15, 20, 30, 60])
    public static let resume = Key("longFiles.resume", "auto", ["auto", "ask", "off"])
    public static let previous = Key("longFiles.previous", "keep", ["keep", "seek", "ignore", "songs"])
    public static let next = Key("longFiles.next", "next", ["next", "seek"])
    public static let seekBack = Key("longFiles.seekBack", 10, [5, 10, 15, 30, 60])
    public static let seekForward = Key("longFiles.seekForward", 30, [10, 15, 30, 60, 120])
    public static let rewind = Key("longFiles.rewind", 10, [0, 5, 10, 15, 30])
    public static let rewindAfter = Key("longFiles.rewindAfter", 5, [1, 5, 10, 30, 60])
    public static let autoRemove = Key("listenLater.autoRemove", true)
    public static let finishedPercent = Key("listenLater.finishedPercent", 90, [80, 90, 95, 100])
    /// Whether a show's page leaves out the episodes already played: set on that page, not in Settings.
    public static let hidePlayed = Key("podcasts.hidePlayed", true)

    /// The ones on the Playback page, which its Reset to defaults clears.
    public static let playback = [
        previousRestart.name, minMinutes.name, resume.name, previous.name, next.name, seekBack.name,
        seekForward.name, rewind.name, rewindAfter.name, autoRemove.name, finishedPercent.name,
    ]

    public struct Key<Value: SyncedValue>: Sendable {
        public let name: String
        public let def: Value
        public let options: [Value]

        init(_ name: String, _ def: Value, _ options: [Value]) {
            self.name = name
            self.def = def
            self.options = options
        }

        /// Its value in the user's settings, or the default while missing or null.
        public func value(in settings: [String: JSON]) -> Value {
            settings[name].flatMap(Value.init(json:)) ?? def
        }
    }
}

extension Synced.Key where Value == Bool {
    init(_ name: String, _ def: Bool) { self.init(name, def, [true, false]) }
}

/// What a synced setting can hold, read from and written as JSON.
public protocol SyncedValue: Hashable, Sendable {
    init?(json: JSON)
    var json: JSON { get }
}

extension Int: SyncedValue {
    public init?(json: JSON) {
        guard let v = json.int else { return nil }
        self = v
    }
    public var json: JSON { JSON(self) }
}

extension Bool: SyncedValue {
    public init?(json: JSON) {
        guard let v = json.bool else { return nil }
        self = v
    }
    public var json: JSON { .bool(self) }
}

extension String: SyncedValue {
    public init?(json: JSON) {
        guard let v = json.string else { return nil }
        self = v
    }
    public var json: JSON { .string(self) }
}
