import SwiftUI

/// Something only the person can fix comes first, under the problem's hero. Its way out is the footer's: Show in
/// Finder when Atlas.app itself is the problem, else Check Again.
struct BlockedScreen: View {
  /// What to fix, in words.
  let reason: String

  /// The problem's hero, and what to fix.
  var body: some View {
    ScreenColumn {
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
