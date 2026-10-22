import AtlasKit
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("StatusReading+Entries")
struct StatusReadingEntriesTests {
  /// A reading of recent activity: an agent's action, the person's, and one that failed, newest first.
  ///
  /// - Returns: The reading.
  /// - Throws: A decoding error, which a fixed reply never gives.
  private func reading() throws -> StatusReading {
    let reply: [String: Any] = [
      "overview": [
        "activity": [
          ["operation": "run", "ok": true, "ageSeconds": 2],
          ["operation": "input", "actor": "viewer", "ok": true, "actionCount": 1, "ageSeconds": 5],
          ["operation": "input", "actor": "viewer", "ok": false, "ageSeconds": 9],
        ]
      ]
    ]
    return StatusReading(try status(reply))
  }

  @Test("a failure is marked failed whoever ran it, and every other action quiet")
  func theMarkFollowsWhetherItWorked() throws {
    let entries = try reading().captionEntries
    #expect(entries.map(\.mark) == [.failed, .quiet, .quiet])
  }

  @Test("the newest is the last caption")
  func theNewestIsMarked() throws {
    #expect(try reading().captionEntries.map(\.isNewest) == [false, false, true])
  }

  @Test("captions and Recent key on the service's row ids, so a newer action never renames the older ones")
  func entriesKeepTheRowsIDs() throws {
    let older: [String: Any] = ["id": "41", "operation": "run", "ok": true, "ageSeconds": 5]
    let newer: [String: Any] = ["id": "42", "operation": "run", "ok": true, "ageSeconds": 1]
    let before = StatusReading(try status(["overview": ["activity": [older]]]))
    let after = StatusReading(try status(["overview": ["activity": [newer, older]]]))
    #expect(before.captionEntries.map(\.id) == ["41"])
    #expect(after.captionEntries.map(\.id) == ["41", "42"])
    #expect(after.recentEntries.map(\.id) == ["42", "41"])
  }
}
