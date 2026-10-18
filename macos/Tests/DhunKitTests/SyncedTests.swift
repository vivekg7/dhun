import Foundation
import Testing

@testable import DhunKit

// Reset to defaults writes null, on this Mac and from the others through sync
// (plan 034). Null must read as the default, not as 0, false or "".
@Test func aResetSettingReadsAsItsDefault() throws {
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: dir) }
    let db = try DB(path: dir.appendingPathComponent("dhun.db").path)

    db.putSetting(Synced.seekBack.name, JSON(60))
    db.putSetting(Synced.previousRestart.name, .bool(false))
    #expect(Synced.seekBack.value(in: db.settings()) == 60)

    for name in Synced.playback { db.putSetting(name, .null) }
    #expect(Synced.seekBack.value(in: db.settings()) == 10)
    #expect(Synced.previousRestart.value(in: db.settings()) == true)
    // A null that reached the map anyway, as an op does before it is stored.
    #expect(Synced.rewind.value(in: [Synced.rewind.name: .null]) == Synced.rewind.def)
}
