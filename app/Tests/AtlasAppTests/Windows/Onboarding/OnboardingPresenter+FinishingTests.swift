import AtlasCommands
import AtlasOnboarding
import AtlasUI
import CoreGraphics
import Testing

@testable import AtlasApp

@Suite("OnboardingPresenter finishing")
struct OnboardingPresenterFinishingTests {
  /// The setup window's sample facts and snapshots, shared by the presenter's suites.
  typealias Samples = OnboardingSamples

  @Test("the finishing check reads Checking…, Passed or Failed, in the words and case every window uses")
  func finishingCheck() throws {
    var snapshot = Samples.snapshot(try Samples.person(Samples.helper()))
    snapshot.finish = .running
    let running = OnboardingPresenter.finishing(snapshot)
    #expect(running.title == "Checking everything")
    #expect(running.loggedIn == "The Atlas account is logged in", "the default helper is named as an account")
    #expect(running.live == .busy && running.liveNote == "Checking…")
    #expect(running.permissionsNote == "Allowed")
    #expect(running.failure == nil)
    snapshot.finish = .passed
    let passed = OnboardingPresenter.finishing(snapshot)
    #expect(passed.title == "Everything works.")
    #expect(passed.live == .done && passed.liveNote == "Passed")
    snapshot.finish = .failed("The test screenshot never arrived")
    let failed = OnboardingPresenter.finishing(snapshot)
    #expect(failed.live == .failed && failed.liveNote == "Failed")
    #expect(failed.title == "Almost there.", "a failed check no longer reads as checking")
    #expect(failed.failure == "The test screenshot never arrived")
  }

  @Test(
    "an agent reads Connected, Gets the Atlas skill, Found or Not on this Mac, and only a found one can be switched"
  )
  func agentRows() throws {
    func agent(_ id: AgentID, found: Bool, connected: Bool) -> AgentChoice {
      var agent = AgentChoice(id: id)
      agent.checked = true
      agent.found = found
      agent.connected = connected
      return agent
    }
    var snapshot = Samples.snapshot(try Samples.person(Samples.helper(), finished: true))
    snapshot.agents = [agent(.claude, found: true, connected: true), agent(.codex, found: true, connected: false)]
    let state = OnboardingPresenter.agents(snapshot)
    #expect(state.agents.map(\.title) == ["Claude Code", "Codex"])
    #expect(state.agents.map(\.detail) == ["Connected", "Found"])
    #expect(state.agents.map(\.isOn) == [true, false], "a connected agent's switch is on")
    snapshot.agents[1].on = true
    #expect(OnboardingPresenter.agents(snapshot).agents[1].detail == "Gets the Atlas skill", "Continue connects it")
    #expect(state.agents.map(\.isEnabled) == [false, true])
    snapshot.agents = [agent(.claude, found: false, connected: false)]
    let missing = try #require(OnboardingPresenter.agents(snapshot).agents.first)
    #expect(missing.detail == "Not on this Mac")
    #expect(!missing.isEnabled)
    snapshot.agents = AgentChoice.all
    snapshot.agentsFailed = true
    let unchecked = OnboardingPresenter.state(snapshot)
    #expect(
      Self.agentsScreen(unchecked)?.agents.map(\.detail) == [
        OnboardingWords.agentNotChecked, OnboardingWords.agentNotChecked,
      ],
      "an agent the failed reading never checked reads unchecked, not missing"
    )
    #expect(unchecked.footer.next?.isEnabled == true, "Continue skips the step")
  }

  @Test("each agent leads with the icon Settings shows for it")
  func agentIcons() throws {
    var snapshot = Samples.snapshot(try Samples.person(Samples.helper(), finished: true))
    snapshot.agents = AgentChoice.all
    #expect(
      OnboardingPresenter.agents(snapshot).agents.map(\.icon) == [.tile(.symbol("asterisk")), .tile(.text(">_"))]
    )
    let pixel = try #require(
      CGContext(
        data: nil,
        width: 1,
        height: 1,
        bitsPerComponent: 8,
        bytesPerRow: 4,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
      )?
      .makeImage()
    )
    let codex = IconImage(id: "/Applications/Codex.app", image: pixel)
    snapshot.agentIcons = [AgentID.codex.rawValue: codex]
    #expect(OnboardingPresenter.agents(snapshot).agents.last?.icon == .app(codex), "the agent's own app icon")
  }

  @Test("while Atlas reads the agents, the line under them says so, and a reading that failed says why instead")
  func checkingAgentsSaysSo() throws {
    var snapshot = Samples.snapshot(try Samples.person(Samples.helper(), finished: true))
    #expect(OnboardingPresenter.note(snapshot) == .working("Checking your agents…"))
    snapshot.agentsLoaded = true
    #expect(OnboardingPresenter.note(snapshot) == .none)
    snapshot.agentsLoaded = false
    snapshot.agentsFailed = true
    snapshot.problem = OnboardingWords.agentsCheckFailed
    #expect(OnboardingPresenter.note(snapshot) == .problem(OnboardingWords.agentsCheckFailed))
  }

  /// The agents screen a state shows, if it shows it.
  ///
  /// - Parameter state: The window's state.
  /// - Returns: The agents screen, or nil.
  private static func agentsScreen(_ state: OnboardingState) -> OnboardingAgentsScreenState? {
    if case .screen(.agents(let screen)) = state.content { return screen }
    return nil
  }
}
