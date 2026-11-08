import Testing

@testable import AtlasOnboarding

@Suite("OnboardingFacts+Watched")
struct OnboardingFactsWatchedTests {
  @Test("the person's account watches its helper's acceptance folder, where the helper's Allow is written")
  func theControllerWatchesTheAcceptanceFolder() throws {
    var side = helper()
    side["uid"] = 502
    #expect(try controller(side).watchedFolders(home: "/Users/theiskaa") == ["/Users/Shared/Atlas-accept-502"])
  }

  @Test("before a reading names the helper's account, the person's account watches no folder")
  func noHelperNoFolder() throws {
    #expect(try controller().watchedFolders(home: "/Users/theiskaa").isEmpty)
    #expect(try controller(helper()).watchedFolders(home: "/Users/theiskaa").isEmpty, "no user ID yet")
  }

  @Test("the helper watches its own job's folder, where the job's socket comes and goes")
  func theHelperWatchesItsJobFolder() throws {
    let facts = try asker(installed: true, running: false, ax: false, screen: false)
    #expect(facts.watchedFolders(home: "/Users/atlas") == ["/Users/atlas/.atlas/helper"])
  }

  @Test("a reading carries the helper's user ID")
  func theHelperUserIDIsRead() throws {
    var side = helper()
    side["uid"] = 502
    #expect(try controller(side).helper.uid == 502)
  }
}
