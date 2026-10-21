import Testing

@testable import AtlasKit

@Suite("OnboardingFacts+Defaults")
struct OnboardingFactsDefaultsTests {
  @Test("the account picked first is one the choice shows: the helper chosen, a new one with no candidates, or none")
  func defaultPick() throws {
    var listed = try controller(helper(answering: false, paired: false))
    listed.candidates = [OnboardingFacts.Candidate(name: "atlas", realName: "Atlas")]
    #expect(listed.defaultPick == "atlas")
    #expect(try controller(helper()).defaultPick == nil, "a helper the choice does not list is never picked")
    #expect(try controller().defaultPick == nil, "a candidate is there to choose")
    var fresh = try controller()
    fresh.candidates = []
    #expect(fresh.defaultPick == "")
    var only = try controller(helper(answering: false, paired: false))
    only.candidates = []
    only.onlyChoice = true
    #expect(only.defaultPick == nil, "no new account while the helper set up before is the one choice")
    #expect(!only.offers(""))
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
