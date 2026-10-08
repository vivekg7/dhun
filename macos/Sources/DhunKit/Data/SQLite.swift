import Foundation
import SQLite3

/// The system's SQLite with as little wrapping as reads well (plan 024: no
/// GRDB). One connection, used from any thread under a lock: the app's
/// writes are a few rows at a time, and the one big write (the first
/// library pull) runs off the main thread.
public final class SQLite: @unchecked Sendable {
    private var db: OpaquePointer?
    private let lock = NSRecursiveLock()
    private var depth = 0

    public init(path: String) throws {
        guard
            sqlite3_open_v2(path, &db, SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_NOMUTEX, nil)
                == SQLITE_OK
        else { throw SQLiteError(message: "open \(path)") }
        try exec("PRAGMA journal_mode = WAL")
        try exec("PRAGMA foreign_keys = OFF")
    }

    deinit { sqlite3_close(db) }

    public func exec(_ sql: String) throws {
        try locked {
            guard sqlite3_exec(db, sql, nil, nil, nil) == SQLITE_OK else { throw error() }
        }
    }

    public func run(_ sql: String, _ args: [SQLValue?] = []) throws {
        try locked {
            let st = try prepare(sql)
            defer { sqlite3_finalize(st) }
            try bind(st, args)
            guard sqlite3_step(st) == SQLITE_DONE else { throw error() }
        }
    }

    /// Runs `sql` once per row, in one transaction.
    public func runMany(_ sql: String, _ rows: [[SQLValue?]]) throws {
        try transaction {
            let st = try prepare(sql)
            defer { sqlite3_finalize(st) }
            for args in rows {
                sqlite3_reset(st)
                sqlite3_clear_bindings(st)
                try bind(st, args)
                guard sqlite3_step(st) == SQLITE_DONE else { throw error() }
            }
        }
    }

    public func query<T>(_ sql: String, _ args: [SQLValue?] = [], _ row: (Row) throws -> T) throws -> [T] {
        try locked {
            let st = try prepare(sql)
            defer { sqlite3_finalize(st) }
            try bind(st, args)
            var out: [T] = []
            while true {
                let rc = sqlite3_step(st)
                if rc == SQLITE_DONE { break }
                guard rc == SQLITE_ROW else { throw error() }
                out.append(try row(Row(st: st!)))
            }
            return out
        }
    }

    /// Nested calls join the outer transaction: SQLite has no nested BEGIN.
    public func transaction<T>(_ body: () throws -> T) throws -> T {
        try locked {
            if depth > 0 { return try body() }
            try exec("BEGIN IMMEDIATE")
            depth += 1
            defer { depth -= 1 }
            do {
                let r = try body()
                try exec("COMMIT")
                return r
            } catch {
                try? exec("ROLLBACK")
                throw error
            }
        }
    }

    public var userVersion: Int {
        get { (try? query("PRAGMA user_version") { $0.int(0) }.first) ?? 0 }
        set { try? exec("PRAGMA user_version = \(newValue)") }
    }

    private func locked<T>(_ body: () throws -> T) rethrows -> T {
        lock.lock()
        defer { lock.unlock() }
        return try body()
    }

    private func prepare(_ sql: String) throws -> OpaquePointer? {
        var st: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &st, nil) == SQLITE_OK else { throw error() }
        return st
    }

    private func bind(_ st: OpaquePointer?, _ args: [SQLValue?]) throws {
        for (i, a) in args.enumerated() {
            let n = Int32(i + 1)
            let rc: Int32
            switch a {
            case nil: rc = sqlite3_bind_null(st, n)
            case .int(let v): rc = sqlite3_bind_int64(st, n, Int64(v))
            case .double(let v): rc = sqlite3_bind_double(st, n, v)
            case .text(let v): rc = sqlite3_bind_text(st, n, v, -1, transient)
            case .blob(let v):
                rc = v.withUnsafeBytes { sqlite3_bind_blob(st, n, $0.baseAddress, Int32(v.count), transient) }
            }
            guard rc == SQLITE_OK else { throw error() }
        }
    }

    private func error() -> SQLiteError { SQLiteError(message: String(cString: sqlite3_errmsg(db))) }

    public struct Row {
        let st: OpaquePointer
        public func int(_ i: Int32) -> Int { Int(sqlite3_column_int64(st, i)) }
        public func bool(_ i: Int32) -> Bool { sqlite3_column_int64(st, i) != 0 }
        public func double(_ i: Int32) -> Double { sqlite3_column_double(st, i) }
        public func text(_ i: Int32) -> String {
            sqlite3_column_text(st, i).map { String(cString: $0) } ?? ""
        }
        public func blob(_ i: Int32) -> Data {
            let n = Int(sqlite3_column_bytes(st, i))
            guard n > 0, let p = sqlite3_column_blob(st, i) else { return Data() }
            return Data(bytes: p, count: n)
        }
    }
}

public enum SQLValue: Sendable {
    case int(Int)
    case double(Double)
    case text(String)
    case blob(Data)
}

extension SQLValue: ExpressibleByIntegerLiteral, ExpressibleByStringLiteral {
    public init(integerLiteral v: Int) { self = .int(v) }
    public init(stringLiteral v: String) { self = .text(v) }
}

public protocol SQLBindable { var sql: SQLValue { get } }
extension Int: SQLBindable { public var sql: SQLValue { .int(self) } }
extension Bool: SQLBindable { public var sql: SQLValue { .int(self ? 1 : 0) } }
extension String: SQLBindable { public var sql: SQLValue { .text(self) } }
extension Double: SQLBindable { public var sql: SQLValue { .double(self) } }
extension Data: SQLBindable { public var sql: SQLValue { .blob(self) } }

/// `[a, b].sql` for `run` and `query` arguments.
extension Array where Element == any SQLBindable {
    public var sql: [SQLValue?] { map { $0.sql } }
}

public struct SQLiteError: LocalizedError, CustomStringConvertible {
    public let message: String
    public var description: String { "SQLite: \(message)" }
    public var errorDescription: String? { description }
}

private let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
