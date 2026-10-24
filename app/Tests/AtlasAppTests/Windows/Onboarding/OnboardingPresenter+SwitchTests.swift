import AtlasUI
import Testing

@testable import AtlasApp

@Suite("OnboardingPresenter switch")
struct OnboardingPresenterSwitchTests {
  /// The setup window's sample facts and snapshots, shared by the presenter's suites.
  typealias Samples = OnboardingSamples

  @Test(
    "the switch screen names the next step until the person switches back",
    arguments: [
      (false, false, true, false, "Next: Lock and Switch, then log in to the Atlas account."),
      (true, false, false, false, "Waiting for Atlas to open in the Atlas account…"),
      (true, false, true, false, "Next: Lock and Switch, then allow Atlas there."),
      (true, true, true, false, "Waiting for the two permissions…"),
      (true, true, false, true, "Waiting for you to switch back…"),
    ] as [(Bool, Bool, Bool, Bool, String)]
  )
  func switchWaiting(signedIn: Bool, answering: Bool, hidden: Bool, allowed: Bool, waiting: String) throws {
    var snapshot = Samples.snapshot(
      try Samples.person(Samples.helper(signedIn: signedIn, answering: answering, hidden: hidden, allowed: allowed))
    )
    #expect(Self.switchScreen(OnboardingPresenter.state(snapshot))?.waiting == waiting)
    snapshot.working = Words.locking
    #expect(Self.switchScreen(OnboardingPresenter.state(snapshot))?.waiting == nil, "the work note shows instead")
  }

  @Test("Lock and Switch shows until the helper is signed in on screen")
  func lockButton() throws {
    let out = try Samples.person(Samples.helper(signedIn: false, answering: false))
    #expect(Self.switchScreen(OnboardingPresenter.state(Samples.snapshot(out)))?.showsLockButton == true)
    let hidden = try Samples.person(Samples.helper(answering: false))
    #expect(Self.switchScreen(OnboardingPresenter.state(Samples.snapshot(hidden)))?.showsLockButton == true)
    let onScreen = try Samples.person(Samples.helper(answering: false, hidden: false))
    #expect(Self.switchScreen(OnboardingPresenter.state(Samples.snapshot(onScreen)))?.showsLockButton == false)
  }

  /// The switch screen a state shows, if it shows it.
  ///
  /// - Parameter state: The window's state.
  /// - Returns: The switch screen, or nil.
  private static func switchScreen(_ state: OnboardingState) -> OnboardingSwitchScreenState? {
    if case .screen(.switchToHelper(let screen)) = state.content { return screen }
    return nil
  }
}
