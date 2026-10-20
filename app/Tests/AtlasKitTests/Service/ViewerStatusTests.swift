import Foundation
import Testing

@testable import AtlasKit

@Suite("ViewerStatus")
struct ViewerStatusTests {
  @Test("a status report decodes the session, the mode, the helper's names, its display and its activity")
  func decodes() throws {
    let reading = try status([
      "ready": true,
      "session": ["user": ["name": "atlastest"], "inputService": ["ok": true, "ready": true]],
      "overview": [
        "mode": "controlling", "account": "atlastest", "accountName": "Atlas Test",
        "display": ["width": 2560.0, "height": 1440.0], "pending": 0,
        "activity": [
          ["operation": "input", "actor": "viewer", "ok": true, "actionCount": 3, "ageSeconds": 42.0]
        ],
      ],
    ])
    #expect(reading.ready)
    #expect(reading.session.userName == "atlastest")
    #expect(reading.session.inputReady)
    #expect(reading.overview.mode == .controlling)
    #expect(reading.overview.accountName == "Atlas Test")
    #expect(reading.overview.display == .init(width: 2560, height: 1440))
    #expect(
      reading.overview.activity == [
        .init(operation: "input", ok: true, actionCount: 3, actor: "viewer", ageSeconds: 42)
      ]
    )
  }

  @Test("missing or mistyped fields fall back to safe defaults, and an unknown mode reads as unavailable")
  func defaults() throws {
    let empty = try status([:])
    #expect(empty.overview.mode == .unavailable)
    #expect(!empty.ready)
    #expect(StatusReading(empty).lastAction == "No recent actions")
    let odd = try status(["ready": "yes", "overview": ["mode": 3, "activity": "none"]])
    #expect(!odd.ready)
    #expect(odd.overview.mode == .unavailable)
    #expect(odd.overview.activity.isEmpty)
    let unknown = try status(["overview": ["mode": "dreaming"]])
    #expect(unknown.overview.mode == .unavailable, "a mode this app does not know reads as unavailable")
  }

  @Test("a failed native-dialog check names the first stage that did not pass")
  func nativeDialogStage() throws {
    let reading = try status([
      "capabilities": [
        "nativeDialogs": [
          "state": "failed", "reason": "No panel",
          "checks": [["stage": "open", "passed": true], ["stage": "save", "passed": false]],
        ]
      ]
    ])
    #expect(reading.capabilities.nativeDialogs.state == .failed)
    #expect(reading.capabilities.nativeDialogs.failedAt == "save")
  }
}
