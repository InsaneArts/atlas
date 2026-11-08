import Testing

@testable import AtlasOnboarding

@Suite("OnboardingFacts+Watched")
struct OnboardingFactsWatchedTests {

  @Test("before a reading names the helper's account, the person's account watches no folder")
  func noHelperNoFolder() throws {
    #expect(try controller().watchedFolders(home: "/Users/theiskaa").isEmpty)
    #expect(try controller(helper()).watchedFolders(home: "/Users/theiskaa").isEmpty, "no user ID yet")
  }

}
