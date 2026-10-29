import AtlasServiceTesting
import AtlasStatus
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("Mode+AgentButton")
struct ModeAgentButtonTests {
  /// The button for an overview in `mode` whose newest action was the given actor's, `age` seconds ago.
  ///
  /// - Parameters:
  ///   - mode: The helper's mode, as the service names it.
  ///   - state: The state the service shows for it.
  ///   - actor: Who acted last, or nil for no action at all.
  ///   - age: How long ago, in seconds.
  ///   - holdingControl: Whether the window asking holds control.
  /// - Returns: The button.
  /// - Throws: A decoding error, which a fixed reply never gives.
  private static func button(
    _ mode: String,
    _ state: String,
    actor: String? = nil,
    age: Double = 0,
    holdingControl: Bool = false
  ) throws -> ViewerAgentButton? {
    let activity = actor.map { [["operation": "input", "ok": true, "actor": $0, "ageSeconds": age]] } ?? []
    let overview = try status(["overview": ["mode": mode, "state": state, "activity": activity]]).overview
    return Mode.seen(by: overview, holdingControl: holdingControl)
      .agentButton(overview, holdingControl: holdingControl, isBusy: false)
  }

  // An agent between two steps still reads as working for a minute (`stopStaysFor`), so the cases sit each side of
  // it; the person's own action and control are what the button must never mistake for an agent at work.
  @Test("Stop shows only while an agent works")
  func stopShowsOnlyWhileAnAgentWorks() throws {
    #expect(try Self.button("agent", "working") == .stop)
    #expect(try Self.button("observing", "idle", actor: "agent", age: 30) == .stop, "between two steps")
    #expect(try Self.button("observing", "idle", actor: "agent", age: 90) == nil, "a minute after its last step")
    #expect(try Self.button("observing", "idle", actor: "viewer", age: 5) == nil, "the person's own input")
    #expect(try Self.button("controlling", "control", holdingControl: true) == nil, "no agent acts in control")
    #expect(try Self.button("stopped", "stopped") == .resume(isEnabled: true), "Resume takes its place")
  }

  @Test("Stop stays while a background job runs after an agent's last step has aged")
  func stopStaysWhileABackgroundJobRuns() throws {
    let aged = [["operation": "job", "ok": true, "actor": "agent", "ageSeconds": 900]]
    let fields: [String: Any] = ["mode": "observing", "state": "idle", "activity": aged, "jobs": ["running": 1]]
    let overview = try status(["overview": fields]).overview
    let mode = Mode.seen(by: overview, holdingControl: false)
    #expect(mode.agentButton(overview, holdingControl: false, isBusy: false) == .stop)
    #expect(mode.agentButton(overview, holdingControl: true, isBusy: false) == .stop, "control ends no job")
  }

  @Test("while the person holds control, Stop shows only for a background job that may still run")
  func holdingControlKeepsStopOnlyForJobs() throws {
    func button(_ jobs: [String: Any]) throws -> ViewerAgentButton? {
      let fields: [String: Any] = ["mode": "controlling", "state": "control", "jobs": jobs]
      let overview = try status(["overview": fields]).overview
      let mode = Mode.seen(by: overview, holdingControl: true)
      return mode.agentButton(overview, holdingControl: true, isBusy: false)
    }
    #expect(try button([:]) == nil, "no agent acts and no job runs")
    #expect(try button(["running": 1]) == .stop)
    #expect(try button(["launching": 1]) == .stop)
    #expect(try button(["unresolved": 1]) == .stop, "a job Atlas cannot read may still run")
  }
}
