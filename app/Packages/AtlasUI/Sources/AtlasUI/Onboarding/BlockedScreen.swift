import SwiftUI

/// Something only the person can fix comes first. Check Again is the footer's.
struct BlockedScreen: View {
  /// What to fix, in words.
  let reason: String

  /// A warning, and what to fix.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(title: OnboardingScreen.blockedTitle, lede: reason) {
        Image(systemName: "exclamationmark.triangle.fill")
          .font(OnboardingLayout.blockedSymbolFont)
          .foregroundStyle(Color.attention)
          .accessibilityHidden(true)
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
