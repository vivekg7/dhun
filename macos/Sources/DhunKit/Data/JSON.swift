import Foundation

/// Any JSON value: synced settings and the ops the outbox keeps
/// (api/openapi.yaml), where the shape depends on the name or the type.
public enum JSON: Codable, Equatable, Sendable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSON])
    case object([String: JSON])

    public init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() {
            self = .null
        } else if let b = try? c.decode(Bool.self) {
            self = .bool(b)
        } else if let n = try? c.decode(Double.self) {
            self = .number(n)
        } else if let s = try? c.decode(String.self) {
            self = .string(s)
        } else if let a = try? c.decode([JSON].self) {
            self = .array(a)
        } else {
            self = .object(try c.decode([String: JSON].self))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .null: try c.encodeNil()
        case .bool(let b): try c.encode(b)
        case .number(let n):
            // Song ids and milliseconds go out as integers, as the server reads them.
            if n == n.rounded(), abs(n) < 9e15 { try c.encode(Int64(n)) } else { try c.encode(n) }
        case .string(let s): try c.encode(s)
        case .array(let a): try c.encode(a)
        case .object(let o): try c.encode(o)
        }
    }

    public var text: String {
        let e = JSONEncoder()
        e.outputFormatting = .sortedKeys
        return (try? String(decoding: e.encode(self), as: UTF8.self)) ?? "null"
    }

    public static func parse(_ text: String) -> JSON {
        (try? JSONDecoder().decode(JSON.self, from: Data(text.utf8))) ?? .null
    }

    public subscript(key: String) -> JSON? {
        if case .object(let o) = self { return o[key] }
        return nil
    }

    public var int: Int? { if case .number(let n) = self { return Int(n) } else { return nil } }
    public var double: Double? { if case .number(let n) = self { return n } else { return nil } }
    public var bool: Bool? { if case .bool(let b) = self { return b } else { return nil } }
    public var string: String? { if case .string(let s) = self { return s } else { return nil } }
    public var array: [JSON]? { if case .array(let a) = self { return a } else { return nil } }
}

extension JSON: ExpressibleByStringLiteral, ExpressibleByIntegerLiteral, ExpressibleByBooleanLiteral,
    ExpressibleByArrayLiteral, ExpressibleByDictionaryLiteral, ExpressibleByFloatLiteral
{
    public init(stringLiteral v: String) { self = .string(v) }
    public init(integerLiteral v: Int) { self = .number(Double(v)) }
    public init(floatLiteral v: Double) { self = .number(v) }
    public init(booleanLiteral v: Bool) { self = .bool(v) }
    public init(arrayLiteral v: JSON...) { self = .array(v) }
    public init(dictionaryLiteral v: (String, JSON)...) {
        self = .object(Dictionary(uniqueKeysWithValues: v))
    }
}

extension JSON {
    public init(_ v: Int) { self = .number(Double(v)) }
    public init(_ v: String) { self = .string(v) }
    public init(_ v: Bool) { self = .bool(v) }
    public init(_ ids: [Int]) { self = .array(ids.map { JSON($0) }) }
}

/// Server times are RFC 3339; the app keeps epoch milliseconds, as the phone does.
public func parseTime(_ s: String?) -> Int {
    guard let s, !s.isEmpty else { return 0 }
    if let d = isoFractional.date(from: s) ?? isoPlain.date(from: s) {
        return Int((d.timeIntervalSince1970 * 1000).rounded())
    }
    return 0
}

public func formatTime(_ ms: Int) -> String {
    isoFractional.string(from: Date(timeIntervalSince1970: Double(ms) / 1000))
}

public func nowMs() -> Int { Int((Date().timeIntervalSince1970 * 1000).rounded()) }

nonisolated(unsafe) private let isoFractional: ISO8601DateFormatter = {
    let f = ISO8601DateFormatter()
    f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return f
}()
nonisolated(unsafe) private let isoPlain = ISO8601DateFormatter()
