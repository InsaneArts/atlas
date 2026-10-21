import Foundation
import Testing

@testable import AtlasKit

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
    #expect(captions.map(\.line) == ["Agent ran a command", "Agent sent 2 actions"])
  }

  @Test("a log-out reads as one action")
  func logOut() throws {
    let reading = try status(["overview": ["activity": [["operation": "logout", "ok": true, "ageSeconds": 3.0]]]])
    #expect(StatusReading(reading).lastAction == "Agent logged out the helper · just now")
  }

  @Test("control waits for agents, recovery and other viewers")
  func controlWaits() throws {
    func overview(_ mode: String) throws -> StatusReading { StatusReading(try status(["overview": ["mode": mode]])) }
    #expect(try overview("agent").controlBlocked == "An agent is acting in the helper.")
    #expect(try overview("controlling").controlBlocked == "Another viewer has control.")
    #expect(try overview("recovering").controlBlocked == "Recover before taking control.")
    #expect(try overview("observing").controlBlocked == nil)
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

  @Test("an unknown check stays unknown, checks read in plain words, and native dialogs show as their own row")
  func statusRows() throws {
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
    #expect(rows.map(\.name) == ["Helper", "Mouse and keyboard", "Accessibility", "Save and Open dialogs"])
    #expect(rows[1].text == CheckWords.notKnown)
    #expect(rows[2].text == "Turn on Atlas in the helper's Accessibility settings.")
    #expect(rows[3].text == "Didn't work 5 min ago.")
    #expect(rows.allSatisfy { !$0.text.contains("atlas ") })
    let stale = try status([
      "checks": [], "capabilities": ["nativeDialogs": ["state": "stale", "reason": "Changed"]],
    ])
    #expect(StatusReading(stale).statusRows.first?.state == .unknown)
    #expect(StatusReading(try status(["checks": []])).statusRows.isEmpty)
  }

  @Test("the session facts say whether this viewer could take control")
  func facts() throws {
    let base: [String: Any] = [
      "overview": ["display": ["width": 2560, "height": 1440], "pending": 0, "mode": "observing"],
      "session": ["inputService": ["ok": true, "ready": true, "pid": 42]],
    ]
    let overview = StatusReading(try status(base))
    #expect(
      overview.facts(holdingControl: false)
        == SessionFacts(display: "2560 × 1440", input: "Ready", control: "Available", pending: "None")
    )
    #expect(overview.facts(holdingControl: true).control == "This window")
    let recovering = StatusReading(try status(["overview": ["mode": "recovering"]]))
    #expect(recovering.facts(holdingControl: true).control == "After recovery", "a held lease is not control now")
    #expect(StatusReading(try status([:])).facts(holdingControl: false).display == Words.unknown)
    for (mode, expected) in [
      ("agent", "Agent working"), ("recovering", "After recovery"), ("blocked", "Not ready"),
      ("unavailable", "Unavailable"), ("loggedOut", "Logged out"),
    ] {
      let reading = try status(["overview": ["mode": mode]])
      #expect(StatusReading(reading).facts(holdingControl: false).control == expected)
    }
  }

  @Test("activity reads as who did what and when")
  func activity() throws {
    let reading = StatusReading(
      try status([
        "overview": [
          "activity": [
            ["operation": "input", "ok": true, "actionCount": 3, "ageSeconds": 3],
            ["operation": "run", "ok": true, "ageSeconds": 40],
            ["operation": "restart", "ok": true, "ageSeconds": 70, "actor": "viewer"],
            ["operation": "input", "ok": false, "actionCount": 8, "ageSeconds": 200],
          ]
        ]
      ])
    )
    let items = reading.activity()
    #expect(
      items.map(\.line) == [
        "Agent sent 3 actions", "Agent ran a command", "You recovered the helper", "Agent couldn't send 8 actions",
      ]
    )
    #expect(items.map(\.when) == ["just now", "40 s ago", "70 s ago", "3 min ago"])
  }

  @Test("a helper that isn't ready gives its failed check's plain words, never the command its step names")
  func blockedReason() throws {
    let reading = StatusReading(
      try status([
        "checks": [["name": "Input service ready", "ok": false, "nextStep": "Run atlas start-input."]],
        "overview": ["mode": "blocked", "reason": "Run atlas start-input."],
      ])
    )
    #expect(reading.reason(holdingControl: false) == "Mouse and keyboard stopped. Recover starts them again.")
  }

  @Test("the reason never shows another viewer's control as yours")
  func reason() throws {
    let held = StatusReading(try status(["overview": ["mode": "controlling", "reason": "A viewer has control"]]))
    #expect(held.reason(holdingControl: false) == Words.anotherViewer)
    #expect(held.reason(holdingControl: true) == "You have mouse and keyboard control")
  }

  @Test("the recovery alert follows recovery, never while holding control")
  func recovery() throws {
    let needs = StatusReading(try status(["overview": ["mode": "recovering", "reason": "An outcome is unknown"]]))
    #expect(
      needs.recovery(holdingControl: false, recovering: false)
        == Recovery(title: "The last action's outcome is unknown", reason: "An outcome is unknown", busy: false)
    )
    let blocked = StatusReading(try status(["overview": ["mode": "blocked", "reason": "x", "recoverable": true]]))
    #expect(blocked.recovery(holdingControl: false, recovering: false)?.title == "The helper isn't ready")
    #expect(needs.recovery(holdingControl: false, recovering: true)?.busy == true)
    let watching = StatusReading(try status(["overview": ["mode": "observing"]]))
    #expect(watching.recovery(holdingControl: false, recovering: true) != nil)
    #expect(watching.recovery(holdingControl: false, recovering: false) == nil)
    #expect(needs.recovery(holdingControl: true, recovering: false) == nil, "never while holding control")
  }

  @Test("a silent helper's checks use the app's words, never a command")
  func silentHelper() throws {
    let reading = try status([
      "checks": [
        [
          "name": "Helper running", "ok": false,
          "nextStep": "Log in to it once: atlas switch locks the screen.",
        ],
        ["name": "Accessibility allowed", "nextStep": "Unknown until the helper is running."],
      ],
      "overview": ["mode": "loggedOut", "reason": "The helper is logged out, so agents can't use it"],
    ])
    let rows = StatusReading(reading).statusRows
    #expect(rows[0].text == "The helper is logged out, so agents can't use it")
    #expect(rows[1].text == CheckWords.notKnown)
    #expect(rows.allSatisfy { !$0.text.contains("atlas ") })
  }
}
