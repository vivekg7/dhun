import Foundation

/// Play speed and pitch (plan 016), independent of each other: a faster
/// speed keeps the voice's pitch, and the pitch moves in semitones, the
/// steps a singer thinks in.
public struct Tempo: Equatable, Sendable {
    public var speed: Double = 1
    public var semitones: Int = 0

    public init(speed: Double = 1, semitones: Int = 0) {
        self.speed = speed
        self.semitones = semitones
    }

    public static let normal = Tempo()
    public static let minSpeed = 0.5
    public static let maxSpeed = 2.0
    public static let maxSemitones = 6

    public var isNormal: Bool { self == .normal }

    public var json: JSON { ["speed": .number(speed), "semitones": JSON(semitones)] }

    /// What the button on Now playing shows, or nil when normal: the speed
    /// ("1.5×"), else the pitch ("+2").
    public var label: String? {
        if speed != 1 { return "\(Tempo.format(speed))×" }
        if semitones > 0 { return "+\(semitones)" }
        if semitones < 0 { return "−\(-semitones)" }
        return nil
    }

    /// A song's setting (`speed.<id>`), or nil when it has none or it cannot be read.
    public static func of(_ v: JSON?) -> Tempo? {
        guard case .object(let o) = v else { return nil }
        let speed = o["speed"]?.double ?? 1
        let semitones = o["semitones"]?.int ?? 0
        return Tempo(
            speed: min(max(speed, minSpeed), maxSpeed),
            semitones: min(max(semitones, -maxSemitones), maxSemitones))
    }

    public static func songSetting(_ song: Int) -> String { "speed.\(song)" }

    /// 1.25 → "1.25", 1.5 → "1.5", 2 → "2", in every locale.
    public static func format(_ speed: Double) -> String {
        let hundredths = Int((speed * 100).rounded())
        if hundredths % 100 == 0 { return "\(hundredths / 100)" }
        var s = String(format: "%d.%02d", hundredths / 100, hundredths % 100)
        while s.hasSuffix("0") { s.removeLast() }
        return s
    }
}
