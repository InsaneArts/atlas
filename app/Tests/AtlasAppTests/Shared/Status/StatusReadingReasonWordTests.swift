import AtlasKit
import Testing

@testable import AtlasApp

@Suite("StatusReading.Reason+Word")
struct StatusReadingReasonWordTests {
  @Test("a helper that isn't ready gives its failed check's plain words, never the command its step names")
  func blockedReason() throws {
    let reading = StatusReading(
      try status([
        "checks": [["name": "Input service ready", "ok": false, "nextStep": "Run atlas start-input."]],
        "overview": ["mode": "blocked", "reason": "Run atlas start-input."],
      ])
    )
    #expect(
      reading.reason(holdingControl: false).word
        == "Mouse and keyboard stopped in the helper. Recover starts them again."
    )
  }

  @Test("this window's control reads as yours, and another window's as in use elsewhere")
  func control() {
    #expect(StatusReading.Reason.yourControl.word == "You have mouse and keyboard control")
    #expect(StatusReading.Reason.inUseElsewhere.word == Words.inUseElsewhereLine)
  }

  @Test("an overview with no reason reads Connecting to the helper")
  func noReason() throws {
    #expect(StatusReading(try status([:])).reason(holdingControl: false).word == "Connecting to the helper")
  }
}
