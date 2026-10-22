import AtlasKit
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
      .windowSummary(reading, paused: false)
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

  @Test("another window's control reads as In use elsewhere, with the line that says why")
  func inUseElsewhere() throws {
    let summary = try Self.shown("controlling", "control")
    #expect(
      summary
        == StatusSummary(label: Words.inUseElsewhere, tone: .quiet, mark: .dot, line: Words.inUseElsewhereLine)
    )
  }
}
