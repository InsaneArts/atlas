#if DEBUG
  import SwiftUI
  import Testing

  @testable import AtlasUI

  @MainActor
  @Suite("OnboardingFooter")
  struct OnboardingFooterTests {
    @Test("a footer button lights up the moment its state lets it be pressed, never later than a press is taken")
    func aButtonLightsUpAsItCanBePressed() {
      #expect(OnboardingFooter.enabling(reduceMotion: false) == Motion.state, "no delay of the bar's own")
      #expect(OnboardingFooter.enabling(reduceMotion: true) == Motion.reduced)
    }
  }
#endif
