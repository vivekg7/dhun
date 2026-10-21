import Foundation
import Testing

@testable import DhunKit

/// A song plays while its cache file fills, so a read past what has arrived
/// must wait for it, tell the fetcher where it waits, and give up with the
/// fetch's error rather than hang.
@Test func readerWaitsForBytesStillArriving() throws {
    let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: url) }
    let file = try SparseFile(url, size: 8)
    let asked = Locked<Int64?>(nil)
    file.wanted = { asked.set($0) }
    file.write(0, Data([1, 2, 3, 4]))

    DispatchQueue.global().asyncAfter(deadline: .now() + 0.1) { file.write(4, Data([5, 6, 7, 8])) }
    var got = [UInt8](repeating: 0, count: 4)
    #expect(try file.read(4, 4, into: &got) == 4)
    #expect(got == [5, 6, 7, 8])
    #expect(asked.get() == 4)
    #expect(file.complete)
}

@Test func readerGivesUpWhenTheFetchFails() throws {
    let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: url) }
    let file = try SparseFile(url, size: 8)
    DispatchQueue.global().asyncAfter(deadline: .now() + 0.1) { file.fail(BytesError.failed("gone")) }
    var got = [UInt8](repeating: 0, count: 4)
    #expect(throws: BytesError.self) { try file.read(0, 4, into: &got) }
}

/// A long file's window rolls behind its reader (plan 037): the bytes from
/// the keep point on move to a new file at their offsets, the rest is
/// forgotten, and reads and writes go on in the new file.
@Test func dropKeepsTheBytesBehindTheReader() throws {
    let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: url) }
    let file = try SparseFile(url, size: 16)
    file.write(0, Data([1, 2, 3, 4, 5, 6, 7, 8]))
    file.write(8, Data([9, 10, 11, 12]))
    file.drop(before: 6)
    #expect(file.firstStart == 6)
    #expect(file.arrived(from: 0) == 0)
    #expect(file.arrived(from: 6) == 12)
    file.write(12, Data([13, 14, 15, 16]))
    var got = [UInt8](repeating: 0, count: 10)
    #expect(try file.read(6, 10, into: &got) == 10)
    #expect(got == [7, 8, 9, 10, 11, 12, 13, 14, 15, 16])
    #expect(!file.complete)
}

final class Locked<T>: @unchecked Sendable {
    private var value: T
    private let lock = NSLock()
    init(_ v: T) { value = v }
    func set(_ v: T) { lock.withLock { value = v } }
    func get() -> T { lock.withLock { value } }
}
