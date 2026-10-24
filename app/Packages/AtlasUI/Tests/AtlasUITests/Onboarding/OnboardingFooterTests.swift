#if DEBUG
  import SwiftUI
  import Testing

  @testable import AtlasUI

  @MainActor
  @Suite("OnboardingFooter")
  struct OnboardingFooterTests {
    @Test("the passed check's Continue lights up 150 milliseconds after the check passes, and nothing else waits")
    func continueFollowsThePassedCheck() throws {
      var passed = try #require(PreviewStates.onboardingFinishingPassed.footer.next)
      #expect(passed.followsSuccess)
      #expect(OnboardingFooter.enabling(passed, reduceMotion: false) == Motion.state.delay(0.15))
      #expect(OnboardingFooter.enabling(passed, reduceMotion: true) == Motion.reduced)
      passed.isEnabled = false
      #expect(OnboardingFooter.enabling(passed, reduceMotion: false) == Motion.state, "it waits again at once")
      let create = try #require(PreviewStates.onboardingCreate.footer.next)
      #expect(OnboardingFooter.enabling(create, reduceMotion: false) == Motion.state)
    }
  }
#endif
