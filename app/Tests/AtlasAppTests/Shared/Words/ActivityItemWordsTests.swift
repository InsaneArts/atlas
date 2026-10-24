import AtlasServiceTesting
import AtlasStatus
import Testing

@testable import AtlasApp

@Suite("ActivityItem+Words")
struct ActivityItemWordsTests {
  @Test("an action reads as who did what, and when")
  func whoDidWhatAndWhen() throws {
    let reading = StatusReading(
      try status([
        "overview": [
          "activity": [
            ["operation": "input", "ok": true, "actionCount": 3, "ageSeconds": 30],
            ["operation": "restart", "ok": true, "ageSeconds": 70, "actor": "viewer"],
            ["operation": "logout", "ok": true, "ageSeconds": 200],
            ["operation": "run", "ok": true],
          ]
        ]
      ])
    )
    let items = reading.activity()
    #expect(
      items.map(\.line) == [
        "Agent sent 3 actions", "You recovered the helper", "Agent logged out the helper", "Agent ran a command",
      ]
    )
    #expect(items.map(\.when) == ["30 s ago", "70 s ago", "3 min ago", ""], "an unknown age reads as nothing")
  }
}
