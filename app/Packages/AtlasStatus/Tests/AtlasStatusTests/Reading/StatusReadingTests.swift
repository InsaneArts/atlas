import AtlasService
import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasStatus

@Suite("StatusReading")
struct StatusReadingTests {
  @Test("captions are the recent actions, oldest first")
  func captions() throws {
    let reading = try status([
      "overview": [
        "activity": [
          ["operation": "input", "ok": true, "actionCount": 2, "ageSeconds": 3.0],
          ["operation": "run", "ok": true, "ageSeconds": 20.0],
          ["operation": "restart", "ok": true, "ageSeconds": 400.0],
        ]
      ]
    ])
    let captions = StatusReading(reading)
      .captions()
    #expect(captions.map(\.row.operation) == ["run", "input"])
  }

  @Test("activity is who did each operation, newest first, by the service's id or else its place")
  func activity() throws {
    let reading = StatusReading(
      try status([
        "overview": [
          "activity": [
            ["id": "a7", "operation": "input", "ok": true, "ageSeconds": 3],
            ["operation": "restart", "ok": false, "ageSeconds": 70, "actor": "viewer"],
          ]
        ]
      ])
    )
    let items = reading.activity()
    #expect(items.map(\.id) == ["a7", "1"])
    #expect(items.map(\.actor) == [.agent, .you])
    #expect(items.map(\.ok) == [true, false])
    #expect(items.map(\.age) == [3, 70])
  }

  @Test("control waits for recovery and other viewers, never for an agent at work")
  func controlWaits() throws {
    func overview(_ mode: String) throws -> StatusReading { StatusReading(try status(["overview": ["mode": mode]])) }
    #expect(try overview("agent").controlBlocked == nil, "the person may take over while an agent works")
    #expect(try overview("controlling").controlBlocked == .anotherWindow)
    #expect(try overview("recovering").controlBlocked == .recovery)
    #expect(try overview("observing").controlBlocked == nil)
    #expect(try overview("stopped").controlBlocked == nil, "the person may take over while agents are stopped")
    #expect(try overview("stopped").availability(holdingControl: false) == .available)
    #expect(try overview("recovering").recoveryOffered(holdingControl: false))
    #expect(try !overview("recovering").recoveryOffered(holdingControl: true))
  }

  @Test(
    "the viewer offers Recover only when it can help, as the panel does: an unknown outcome, or input Recover starts",
    arguments: [
      ("recovering", false, true),
      ("blocked", true, true),
      ("blocked", false, false),
    ]
  )
  func recoveryOnlyWhenItHelps(mode: String, recoverable: Bool, offered: Bool) throws {
    let reading = StatusReading(try status(["overview": ["mode": mode, "recoverable": recoverable]]))
    #expect((reading.recovery(holdingControl: false, recovering: false) != nil) == offered)
  }

  @Test("an unknown outcome offers no recovery alert when a prerequisite prevents recovery")
  func recoveryWaitsForPrerequisites() throws {
    let reading = StatusReading(
      try status([
        "overview": ["mode": "recovering", "state": "needsYou", "pending": 1, "recoveryHelps": false]
      ])
    )
    #expect(reading.controlBlocked == .recovery)
    #expect(!reading.recoveryOffered(holdingControl: false))
    #expect(reading.recovery(holdingControl: false, recovering: false) == nil)
  }

  @Test("an unknown check stays unknown, a failed one keeps its step for the app, and native dialogs are their own row")
  func statusRows() throws {
    let accessibility = ViewerStatus.Check(
      name: "Accessibility allowed",
      ok: false,
      nextStep: "In the helper's System Settings, turn Atlas on (atlas init walks you through it)."
    )
    let reading = try status([
      "checks": [
        ["name": "Helper running", "ok": true],
        ["name": "Input service ready", "ok": NSNull(), "nextStep": "Run atlas start-input."],
        ["name": accessibility.name, "ok": false, "nextStep": accessibility.nextStep ?? ""],
      ],
      "overview": ["mode": "blocked"],
      "capabilities": [
        "nativeDialogs": [
          "state": "failed", "ageSeconds": 300, "reason": "Native Save/Open did not complete in this session",
          "checks": [
            ["kind": "save", "passed": false, "stage": "save confirmation enabled"],
            ["kind": "open", "passed": false, "stage": "open confirmation enabled"],
          ],
        ]
      ],
    ])
    let rows = StatusReading(reading).statusRows
    #expect(rows.map(\.state) == [.ready, .unknown, .bad, .bad])
    #expect(
      rows.map(\.subject) == [
        .check("Helper running"), .check("Input service ready"), .check("Accessibility allowed"), .nativeDialogs,
      ]
    )
    #expect(rows.map(\.detail) == [.passed, .notKnown, .failed(accessibility), .didNotWork(ageSeconds: 300)])
    let stale = try status([
      "checks": [], "capabilities": ["nativeDialogs": ["state": "stale", "reason": "Changed"]],
    ])
    #expect(StatusReading(stale).statusRows.first?.state == .unknown)
    #expect(StatusReading(try status(["checks": []])).statusRows.isEmpty)
  }

  @Test("a silent helper's failed checks stand on its overview's reason, and its unknown ones stay unknown")
  func silentHelper() throws {
    let reading = try status([
      "checks": [
        ["name": "Helper running", "ok": false, "nextStep": "Log in to it once: atlas switch locks the screen."],
        ["name": "Accessibility allowed", "nextStep": "Unknown until the helper is running."],
      ],
      "overview": ["mode": "loggedOut", "reason": "The helper is logged out"],
    ])
    let rows = StatusReading(reading).statusRows
    #expect(rows.map(\.detail) == [.helperSilent("The helper is logged out"), .notKnown])
  }

  @Test("a reading that holds control reports the helper ready, without words")
  func holdingControl() throws {
    let reading = StatusReading(
      try status([
        "overview": ["display": ["width": 2560, "height": 1440], "pending": 2, "mode": "controlling"],
        "session": ["inputService": ["ok": true, "ready": true, "pid": 42]],
      ])
    )
    #expect(
      reading.facts(holdingControl: true)
        == SessionFacts(
          display: .init(width: 2560, height: 1440),
          input: .ready,
          control: .thisWindow,
          pending: 2
        )
    )
  }

  @Test("the session facts say whether input answers and whether this viewer could take control")
  func facts() throws {
    let reachable = try status(["session": ["inputService": ["ok": true, "ready": false, "pid": 42]]])
    #expect(StatusReading(reachable).facts(holdingControl: false).input == .notReady)
    #expect(StatusReading(try status([:])).facts(holdingControl: false).input == .notAnswering)
    let recovering = StatusReading(try status(["overview": ["mode": "recovering"]]))
    #expect(recovering.facts(holdingControl: true).control == .afterRecovery, "a held lease is not control now")
    for (mode, expected) in [
      ("observing", SessionFacts.Control.available), ("agent", .available), ("controlling", .anotherWindow),
      ("recovering", .afterRecovery), ("blocked", .notReady), ("unavailable", .notReady), ("loggedOut", .loggedOut),
    ] {
      let reading = try status(["overview": ["mode": mode]])
      #expect(StatusReading(reading).facts(holdingControl: false).control == expected)
    }
  }

  @Test("a helper that isn't ready gives its failed check, never the step its reason names")
  func blockedReason() throws {
    let check = ViewerStatus.Check(name: "Input service ready", ok: false, nextStep: "Run atlas start-input.")
    let reading = StatusReading(
      try status([
        "checks": [["name": check.name, "ok": false, "nextStep": check.nextStep ?? ""]],
        "overview": ["mode": "blocked", "reason": "Run atlas start-input."],
      ])
    )
    #expect(reading.reason(holdingControl: false) == .failedCheck(check))
  }

  @Test("the reason never shows another window's control as yours")
  func reason() throws {
    let held = StatusReading(try status(["overview": ["mode": "controlling", "reason": "A viewer has control"]]))
    #expect(held.reason(holdingControl: false) == .inUseElsewhere)
    #expect(held.reason(holdingControl: true) == .yourControl)
  }

  @Test("this viewer's control while an agent's command runs reads as control beside that command")
  func reasonBesideACommand() throws {
    let held = StatusReading(try status(["overview": ["mode": "controlling", "commands": 1]]))
    #expect(held.reason(holdingControl: true) == .yourControlBesideCommand)
    #expect(held.reason(holdingControl: false) == .inUseElsewhere)
  }

  @Test("an idle helper's reason is the newest agent action, else that nothing is happening")
  func idleReason() throws {
    let acted = StatusReading(
      try status([
        "overview": [
          "state": "idle",
          "activity": [
            ["operation": "input", "actor": "viewer", "ok": true, "ageSeconds": 1],
            ["id": "a1", "operation": "run", "ok": true, "ageSeconds": 120],
          ],
        ]
      ])
    )
    #expect(acted.reason(holdingControl: false) == .lastAction(try #require(acted.newestAgentAction)))
    #expect(acted.newestAgentAction?.id == "a1", "the person's own action is not an agent's")
    let quiet = StatusReading(try status(["overview": ["state": "idle"]]))
    #expect(quiet.reason(holdingControl: false) == .nothingHappening)
  }

  @Test("a reason the service has not given, or gave empty, is connecting")
  func noReason() throws {
    #expect(StatusReading(try status([:])).reason(holdingControl: false) == .connecting)
    let empty = StatusReading(try status(["overview": ["reason": ""]]))
    #expect(empty.reason(holdingControl: false) == .connecting)
    let given = StatusReading(try status(["overview": ["reason": "Waiting"]]))
    #expect(given.reason(holdingControl: false) == .service("Waiting"))
  }

  @Test("the recovery alert follows recovery, never while holding control")
  func recovery() throws {
    let needs = StatusReading(try status(["overview": ["mode": "recovering", "reason": "An outcome is unknown"]]))
    #expect(
      needs.recovery(holdingControl: false, recovering: false) == .unknownOutcome(.service("An outcome is unknown"))
    )
    let blocked = StatusReading(try status(["overview": ["mode": "blocked", "reason": "x", "recoverable": true]]))
    #expect(blocked.recovery(holdingControl: false, recovering: false) == .notReady(.service("x")))
    #expect(needs.recovery(holdingControl: false, recovering: true)?.busy == true)
    let watching = StatusReading(try status(["overview": ["mode": "observing"]]))
    #expect(watching.recovery(holdingControl: false, recovering: true) == .running)
    #expect(watching.recovery(holdingControl: false, recovering: false) == nil)
    #expect(needs.recovery(holdingControl: true, recovering: false) == nil, "never while holding control")
  }

  @Test("Needs you is input that stopped only while the helper is blocked")
  func inputStopped() throws {
    #expect(StatusReading(try status(["overview": ["mode": "blocked"]])).inputStopped)
    #expect(!StatusReading(try status(["overview": ["mode": "recovering"]])).inputStopped)
  }
}
