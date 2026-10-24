import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasService

@Suite("ViewerStatus")
struct ViewerStatusTests {
  @Test("the deleted accounts' open logins and a silent helper's login are read, and none by default")
  func leftoverLogins() throws {
    let reading = try status(["leftoverLogins": [502, 504], "helperStale": true])
    #expect(reading.leftoverLogins == [502, 504])
    #expect(reading.helperStale)
    let older = try status(["ready": true])
    #expect(older.leftoverLogins.isEmpty)
    #expect(!older.helperStale)
  }

  @Test("a status report decodes the session, the mode, the helper's names, its display and what it did")
  func decodes() throws {
    let reading = try status([
      "ready": true,
      "session": ["user": ["name": "atlastest"], "inputService": ["ok": true, "ready": true]],
      "overview": [
        "mode": "controlling", "state": "control", "label": "You have control", "recoverable": true,
        "account": "atlastest", "accountName": "Atlas Test",
        "display": ["width": 2560.0, "height": 1440.0], "pending": 0,
        "activity": [
          ["operation": "input", "actor": "viewer", "ok": true, "actionCount": 3, "ageSeconds": 42.0],
          ["operation": "input", "ok": true, "says": "clicked Save in TextEdit"],
          ["operation": "run", "ok": true, "says": ""],
        ],
      ],
    ])
    #expect(reading.ready)
    #expect(reading.session.userName == "atlastest")
    #expect(reading.session.inputReady)
    #expect(reading.overview.mode == .controlling)
    #expect(reading.overview.state == .control)
    #expect(reading.overview.label == "You have control")
    #expect(reading.overview.recoverable)
    #expect(reading.overview.accountName == "Atlas Test")
    #expect(reading.overview.display == .init(width: 2560, height: 1440))
    #expect(
      reading.overview.activity == [
        .init(operation: "input", ok: true, actionCount: 3, actor: "viewer", ageSeconds: 42),
        .init(operation: "input", ok: true, says: "clicked Save in TextEdit"),
        .init(operation: "run", ok: true),
      ]
    )
  }

  @Test("Stop is offered while an agent acts or acted a minute ago, and Resume once stopped")
  func stopAndResume() throws {
    func overview(_ mode: String, agentAge: Double? = nil, actor: String = "agent") throws -> ViewerStatus.Overview {
      let rows = agentAge.map { [["operation": "input", "ok": true, "actor": actor, "ageSeconds": $0]] } ?? []
      return try status(["overview": ["mode": mode, "activity": rows]]).overview
    }
    #expect(try overview("agent").offersStop)
    #expect(try overview("observing", agentAge: 30).offersStop, "between two steps")
    #expect(try !overview("observing", agentAge: 90).offersStop)
    #expect(try !overview("observing", agentAge: 5, actor: "viewer").offersStop, "the person's own input")
    #expect(try !overview("stopped").offersStop)
    #expect(try overview("stopped").offersResume)
    #expect(try !overview("agent").offersResume)
  }

  @Test(
    "Recover may run only where the command line takes it: never under a viewer's control, an agent's work under way, or an unreachable helper",
    arguments: [
      ("observing", 0, true),
      ("blocked", 0, true),
      ("agent", 0, true),
      ("agent", 1, false),
      ("stopped", 0, true),
      ("stopped", 1, false),
      ("controlling", 0, false),
      ("unavailable", 0, false),
    ]
  )
  func recoverMayRun(mode: String, inFlight: Int, runs: Bool) throws {
    let reading = try status(["overview": ["mode": mode, "inFlight": inFlight]])
    #expect(reading.overview.recoverMayRun == runs)
  }

  @Test("missing or mistyped fields fall back to safe defaults, and an unknown mode reads as unavailable")
  func defaults() throws {
    let empty = try status([:])
    #expect(empty.overview.mode == .unavailable)
    #expect(empty.overview.state == .notReady)
    #expect(!empty.overview.recoverable)
    #expect(!empty.ready)
    #expect(empty.overview.activity.isEmpty)
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
