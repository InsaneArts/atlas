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

  @Test("a failure reads as failed whoever ran it, then the person's own as yours, and an agent's as others'")
  func theKindFollowsWhoAndWhetherItWorked() throws {
    let entries = try reading().recentEntries
    #expect(entries.map(\.kind) == [.others, .yours, .failed])
    #expect(entries.map(\.initial) == ["A", "Y", "Y"])
  }

  @Test("the newest is the first row of the sidebar and the last caption")
  func theNewestIsMarked() throws {
    #expect(try reading().recentEntries.map(\.isNewest) == [true, false, false])
    #expect(try reading().captionEntries.map(\.isNewest) == [false, false, true])
  }
}
