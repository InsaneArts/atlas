import AtlasUI
import Testing

@testable import AtlasApp

@Suite("OnboardingPresenter switch")
struct OnboardingPresenterSwitchTests {
  /// The setup window's sample facts and snapshots, shared by the presenter's suites.
  typealias Samples = OnboardingSamples

  @Test(
    "the switch screen says nothing until the person has logged in to the helper, then what Atlas waits for",
    arguments: [
      (false, false, true, false, nil),
      (true, false, false, false, "Waiting for Atlas to open in the Atlas account…"),
      (true, false, true, false, "Waiting for Atlas to open in the Atlas account…"),
      (true, true, true, false, "Waiting for the two permissions…"),
      (true, true, false, true, "Waiting for you to switch back…"),
    ] as [(Bool, Bool, Bool, Bool, String?)]
  )
  func switchWaiting(signedIn: Bool, answering: Bool, hidden: Bool, allowed: Bool, waiting: String?) throws {
    var snapshot = Samples.snapshot(
      try Samples.person(Samples.helper(signedIn: signedIn, answering: answering, hidden: hidden, allowed: allowed))
    )
    #expect(Self.switchScreen(OnboardingPresenter.state(snapshot))?.waiting == waiting)
    snapshot.working = Words.locking
    #expect(Self.switchScreen(OnboardingPresenter.state(snapshot))?.waiting == nil, "the work note shows instead")
  }

  @Test("the first step names the helper to log in to, by its given name when it has a long one")
  func logInStep() throws {
    let atlas = try Samples.person(Samples.helper(signedIn: false, answering: false))
    let short = try #require(Self.switchScreen(OnboardingPresenter.state(Samples.snapshot(atlas))))
    #expect(short.logInTitle == "Log in to the Atlas account")
    var long = Samples.helper(signedIn: false, answering: false)
    long["realName"] = "Maria Fernanda Montgomery-Castellanos de la Vega"
    let named = try #require(
      Self.switchScreen(OnboardingPresenter.state(Samples.snapshot(try Samples.person(long))))
    )
    #expect(named.logInTitle == "Log in to Maria Fernanda")
    #expect(named.title == "Now log in to Maria Fernanda.")
  }

  @Test("Lock and Switch shows until the helper is signed in on screen")
  func lockButton() throws {
    let out = try Samples.person(Samples.helper(signedIn: false, answering: false))
    #expect(OnboardingPresenter.state(Samples.snapshot(out)).footer.next?.event == .lockAndSwitch)
    let hidden = try Samples.person(Samples.helper(answering: false))
    #expect(OnboardingPresenter.state(Samples.snapshot(hidden)).footer.next?.event == .lockAndSwitch)
    let onScreen = try Samples.person(Samples.helper(answering: false, hidden: false))
    #expect(OnboardingPresenter.state(Samples.snapshot(onScreen)).footer.next == nil)
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
