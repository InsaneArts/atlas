import Testing

@testable import AtlasUI

@Suite("OnboardingScreen+ID")
struct OnboardingScreenIDTests {
  @Test("a change inside a screen keeps its id; a new blocked reason is a new screen")
  func identityFollowsTheScreen() {
    let waiting = OnboardingDoneScreenState(lede: "Log in to Atlas after a restart.", opensAtLogin: true)
    var switched = waiting
    switched.opensAtLogin = false
    #expect(OnboardingScreen.done(waiting).id == OnboardingScreen.done(switched).id)
    #expect(OnboardingScreen.blocked("Move Atlas.app").id != OnboardingScreen.blocked("Log in first").id)
  }
}
