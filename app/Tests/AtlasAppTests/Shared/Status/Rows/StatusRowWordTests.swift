import AtlasServiceTesting
import AtlasStatus
import Foundation
import Testing

@testable import AtlasApp

@Suite("StatusRow.Subject+Word, StatusRow.Detail+Word")
struct StatusRowWordTests {
  @Test("checks read in plain words, never a command, and native dialogs as their own row")
  func plainWords() throws {
    let reading = try status([
      "checks": [
        ["name": "Helper running", "ok": true],
        ["name": "Input service ready", "ok": NSNull(), "nextStep": "Run atlas start-input."],
        [
          "name": "Accessibility allowed", "ok": false,
          "nextStep": "In the helper's System Settings, turn Atlas on (atlas init walks you through it).",
        ],
      ],
      "overview": ["mode": "blocked"],
      "capabilities": ["nativeDialogs": ["state": "failed", "ageSeconds": 300, "reason": "Did not complete"]],
    ])
    let rows = StatusReading(reading).statusRows
    #expect(rows.map(\.subject.word) == ["Helper", "Mouse and keyboard", "Accessibility", "Save and Open dialogs"])
    #expect(rows[1].detail.word == CheckWords.notKnown)
    #expect(rows[2].detail.word == "Turn on Atlas in the helper's Accessibility settings.")
    #expect(rows[3].detail.word == "Didn't work 5 min ago.")
    #expect(rows.allSatisfy { !$0.detail.word.contains("atlas ") })
  }

  @Test("a silent helper's checks use the overview's reason, and Connecting to the helper before it gives one")
  func silentHelper() {
    #expect(StatusRow.Detail.helperSilent("The helper is logged out").word == "The helper is logged out")
    #expect(StatusRow.Detail.helperSilent(nil).word == Words.connecting)
  }
}
