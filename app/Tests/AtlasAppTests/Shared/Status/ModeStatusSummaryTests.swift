import AtlasServiceTesting
import AtlasStatus
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("Mode+StatusSummary")
struct ModeStatusSummaryTests {
  /// What the window shows for an overview, as a viewer holding control or not.
  ///
  /// - Parameters:
  ///   - mode: The helper's mode, as the service names it.
  ///   - state: The state the service shows for it.
  ///   - holdingControl: Whether this window holds control.
  /// - Returns: The window's status.
  /// - Throws: A decoding error, which a fixed reply never gives.
  private static func shown(_ mode: String, _ state: String, holdingControl: Bool = false) throws -> StatusSummary {
    let reading = StatusReading(try status(["overview": ["mode": mode, "state": state]]))
    return Mode.seen(by: reading.status.overview, holdingControl: holdingControl)
      .windowSummary(reading)
  }

  /// Every mode a window can show, each from the overview that gives it.
  ///
  /// - Returns: One reading and mode for each row of the status table.
  /// - Throws: A decoding error, which a fixed reply never gives.
  private static func everyMode() throws -> [(kind: Mode.Kind, reading: StatusReading, mode: Mode)] {
    let rows: [(String, String, Bool)] = [
      ("observing", "idle", false), ("agent", "working", false), ("controlling", "control", true),
      ("recovering", "needsYou", false), ("stopped", "stopped", false), ("unavailable", "notReady", false),
      ("loggedOut", "notReady", false), ("controlling", "control", false),
    ]
    var modes = try rows.map { mode, state, holding in
      let reading = StatusReading(try status(["overview": ["mode": mode, "state": state]]))
      let seen = Mode.seen(by: reading.status.overview, holdingControl: holding)
      return (kind: seen.kind, reading: reading, mode: seen)
    }
    let first = StatusReading(try status([:]))
    modes.append((kind: .connecting, reading: first, mode: .connecting))
    return modes
  }

  @Test("every state has a line in the panel, and in the main window all but Logged out, whose stage says it")
  func everyStateHasALine() throws {
    let modes = try Self.everyMode()
    #expect(Set(modes.map(\.kind)) == Set(Mode.Kind.allCases), "every row of the table is covered")
    for (kind, reading, mode) in modes {
      #expect(!mode.panelSummary(reading, paused: false).line.isEmpty, "\(kind) in the panel")
      #expect(mode.windowSummary(reading).line.isEmpty == (kind == .loggedOut), "\(kind) in the main window")
    }
  }

  @Test("a fresh helper reads Ready for your agents, and connecting says whether it starts or reconnects")
  func freshAndConnectingLines() throws {
    let idle = StatusReading(try status(["overview": ["mode": "observing", "state": "idle"]]))
    let mode = Mode.seen(by: idle.status.overview, holdingControl: false)
    #expect(mode.windowSummary(idle).line == "Ready for your agents")
    #expect(mode.panelSummary(idle, paused: false).line == "Ready for your agents")
    let first = StatusReading(try status([:]))
    #expect(Mode.connecting.windowSummary(first).line == "Starting the helper…")
    #expect(Mode.connecting.panelSummary(first, paused: false, reconnecting: true).line == "Reconnecting…")
  }

  @Test("the panel and the main window say the same line while the person holds control")
  func controlReadsTheSameInBoth() throws {
    let reading = StatusReading(try status(["overview": ["mode": "controlling", "state": "control"]]))
    let mode = Mode.seen(by: reading.status.overview, holdingControl: true)
    #expect(mode.panelSummary(reading, paused: false).line == "The agent waits")
    #expect(mode.windowSummary(reading).line == "The agent waits")
  }

  @Test(
    "every overview state reads with its word, tone and mark",
    arguments: [
      ("agent", "working", "Working", StatusTone.active, StatusMark.pulsing),
      ("observing", "idle", "Idle", .quiet, .dot),
      ("recovering", "needsYou", "Needs you", .attention, .dot),
      ("stopped", "stopped", "Stopped", .attention, .square),
      ("unavailable", "notReady", "Not ready", .quiet, .small),
      ("loggedOut", "notReady", "Logged out", .quiet, .dot),
    ]
  )
  func everyState(mode: String, state: String, label: String, tone: StatusTone, mark: StatusMark) throws {
    let summary = try Self.shown(mode, state)
    #expect(summary.label == label)
    #expect(summary.tone == tone)
    #expect(summary.mark == mark)
  }

  @Test("holding control reads as You have control, in the link colour, with the agent waiting")
  func holdingControl() throws {
    let summary = try Self.shown("controlling", "control", holdingControl: true)
    #expect(summary == StatusSummary(label: "You have control", tone: .link, mark: .dot, line: Words.controlLine))
  }

  @Test("holding control while an agent's command runs says so in the panel, the main window and its reason")
  func controlBesideARunningCommand() throws {
    let reading = StatusReading(
      try status(["overview": ["mode": "controlling", "state": "control", "commands": 1]])
    )
    let mode = Mode.seen(by: reading.status.overview, holdingControl: true)
    #expect(mode.panelSummary(reading, paused: false).line == "A command an agent started is still running")
    #expect(mode.windowSummary(reading).line == "A command an agent started is still running")
    #expect(reading.reason(holdingControl: true).word == Words.yourControlBesideCommand)
  }

  @Test("an unknown outcome while an agent's command runs says recovery waits for it, and Recover doesn't help")
  func needsYouBesideARunningCommand() throws {
    let reading = StatusReading(
      try status([
        "overview": ["mode": "recovering", "state": "needsYou", "pending": 1, "commands": 1, "recoveryHelps": true]
      ])
    )
    let mode = Mode.seen(by: reading.status.overview, holdingControl: false)
    #expect(mode.panelSummary(reading, paused: false).line == Words.unfinishedBesideCommandLine)
    #expect(mode.windowSummary(reading).line == Words.unfinishedBesideCommandLine)
    #expect(!reading.status.overview.recoveryHelps)
  }

  @Test("another window's control reads as In use elsewhere, with the line that says why")
  func inUseElsewhere() throws {
    let summary = try Self.shown("controlling", "control")
    #expect(
      summary
        == StatusSummary(label: Words.inUseElsewhere, tone: .quiet, mark: .dot, line: Words.inUseElsewhereLine)
    )
  }
}
