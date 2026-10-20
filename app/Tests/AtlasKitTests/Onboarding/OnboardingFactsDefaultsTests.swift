import Testing

@testable import AtlasKit

@Suite("OnboardingFacts+Defaults")
struct OnboardingFactsDefaultsTests {
  @Test("the account picked first is the helper chosen, else a new one with no candidates, else none")
  func defaultPick() throws {
    #expect(try controller(helper()).defaultPick == "atlas")
    #expect(try controller().defaultPick == nil, "a candidate is there to choose")
    var fresh = try controller()
    fresh.candidates = []
    #expect(fresh.defaultPick == "")
  }

  @Test("Open at Login keeps the person's earlier choice, and the first setup turns it on")
  func opensAtLogin() throws {
    var facts = try controller()
    #expect(facts.opensAtLogin(enabledNow: false), "the first setup turns it on")
    facts.loginDecided = true
    #expect(!facts.opensAtLogin(enabledNow: false))
    #expect(facts.opensAtLogin(enabledNow: true))
  }
}
