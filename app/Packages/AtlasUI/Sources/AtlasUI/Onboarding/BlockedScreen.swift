import SwiftUI

/// Something only the person can fix comes first, under the problem's hero (v3 D5). Check Again is the footer's.
struct BlockedScreen: View {
  /// What to fix, in words.
  let reason: String

  /// The problem's hero, and what to fix.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(title: OnboardingScreen.blockedTitle, lede: reason) {
        HeroDisc(kind: .problem)
      }
    }
  }
}

#if DEBUG
  #Preview("Blocked") {
    BlockedScreen(reason: PreviewStates.onboardingBlockedReason)
      .onboardingSample()
  }
#endif
