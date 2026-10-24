import AtlasKit
import Testing

@testable import AtlasApp

@Suite("StatusReading+Lines")
struct StatusReadingLinesTests {
  @Test("the lines beside Working and Idle name the newest agent action, never the person's own, else say no more")
  func workingAndIdleLines() throws {
    let reading = StatusReading(
      try status([
        "overview": [
          "reason": "An agent is acting in the helper.",
          "activity": [
            ["operation": "input", "actor": "viewer", "ok": true, "actionCount": 3, "ageSeconds": 1],
            ["operation": "input", "ok": true, "ageSeconds": 120, "says": "turning Show Center off in Opie"],
          ],
        ]
      ])
    )
    #expect(reading.workingLine(withAge: false) == "Turning Show Center off in Opie")
    #expect(reading.workingLine(withAge: true) == "Turning Show Center off in Opie · 2 min ago")
    #expect(reading.idleLine == "Last: turning Show Center off in Opie · 2 min ago")
    let quiet = StatusReading(try status(["overview": ["reason": "An agent is acting in the helper."]]))
    #expect(quiet.workingLine(withAge: true) == "An agent is acting in the helper", "a status line, not a sentence")
    #expect(quiet.idleLine.isEmpty)
  }

  @Test("Working with no action and no reason yet reads as connecting")
  func workingBeforeAnyReason() throws {
    #expect(StatusReading(try status([:])).workingLine(withAge: true) == Words.connecting)
  }
}
