import AtlasCommands
import AtlasOnboarding
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("OnboardingPresenter finishing")
struct OnboardingPresenterFinishingTests {
  /// The setup window's sample facts and snapshots, shared by the presenter's suites.
  typealias Samples = OnboardingSamples

  @Test("the finishing check reads checking, passed or failed, with Try Again on failure")
  func finishingCheck() throws {
    var snapshot = Samples.snapshot(try Samples.person(Samples.helper()))
    snapshot.finish = .running
    let running = OnboardingPresenter.finishing(snapshot)
    #expect(running.title == "Checking everything")
    #expect(running.loggedIn == "The Atlas account is logged in", "the default helper is named as an account")
    #expect(running.live == .busy && running.liveNote == "checking…")
    #expect(running.failure == nil)
    snapshot.finish = .passed
    let passed = OnboardingPresenter.finishing(snapshot)
    #expect(passed.title == "Everything works.")
    #expect(passed.live == .done && passed.liveNote == "passed")
    snapshot.finish = .failed("The test screenshot never arrived")
    let failed = OnboardingPresenter.finishing(snapshot)
    #expect(failed.live == .failed && failed.liveNote == "failed")
    #expect(failed.title == "Almost there.", "a failed check no longer reads as checking")
    #expect(failed.failure == "The test screenshot never arrived")
  }

  @Test("an agent reads connected, found or not installed, and only a found, unconnected one can be switched")
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
    #expect(state.agents.map(\.detail) == [OnboardingWords.agentConnected, OnboardingWords.agentFound])
    #expect(state.agents.map(\.isOn) == [true, false], "a connected agent's switch is on")
    #expect(state.agents.map(\.isEnabled) == [false, true])
    snapshot.agents = [agent(.claude, found: false, connected: false)]
    let missing = try #require(OnboardingPresenter.agents(snapshot).agents.first)
    #expect(missing.detail == OnboardingWords.agentMissing)
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

  /// The agents screen a state shows, if it shows it.
  ///
  /// - Parameter state: The window's state.
  /// - Returns: The agents screen, or nil.
  private static func agentsScreen(_ state: OnboardingState) -> OnboardingAgentsScreenState? {
    if case .screen(.agents(let screen)) = state.content { return screen }
    return nil
  }
}
