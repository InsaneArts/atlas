import Testing

@testable import AtlasOnboarding

@Suite("OnboardingFacts.Helper+SwitchProgress")
struct SwitchProgressTests {
  @Test("the three switch steps advance as the helper signs in, answers and is allowed")
  func progress() throws {
    let signedOut = try controller(helper(signedIn: false, answering: false, allowed: false)).helper
    #expect(signedOut.switchProgress == SwitchProgress(logIn: .current, openAtlas: .waiting, allow: .waiting))
    let silent = try controller(helper(answering: false, allowed: false)).helper
    #expect(silent.switchProgress == SwitchProgress(logIn: .done, openAtlas: .current, allow: .waiting))
    let unallowed = try controller(helper(allowed: false)).helper
    #expect(unallowed.switchProgress == SwitchProgress(logIn: .done, openAtlas: .done, allow: .current))
    let allowed = try controller(helper(hidden: false, allowed: true)).helper
    #expect(allowed.switchProgress == SwitchProgress(logIn: .done, openAtlas: .done, allow: .done))
  }
}
