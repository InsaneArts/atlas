import SwiftUI

/// Something only the person can fix comes first.
struct BlockedScreen: View {
  /// What to fix, in words.
  let reason: String
  /// Receives Check Again.
  let send: @MainActor (OnboardingEvent) -> Void

  /// A warning, what to fix, and Check Again.
  var body: some View {
    ScreenColumn {
      Image(systemName: "exclamationmark.triangle.fill")
        .font(.system(size: OnboardingLayout.blockedSymbol))
        .foregroundStyle(Color.attention)
        .accessibilityHidden(true)
      Headline(title: "One thing first.")
      Lede(text: reason)
      Button("Check Again") { send(.checkAgain) }
        .buttonStyle(.capsule(size: .large))
        .padding(.top, Spacing.m)
    }
  }
}

#if DEBUG
  #Preview("Blocked") {
    BlockedScreen(reason: PreviewStates.onboardingBlockedReason) { _ in }
      .onboardingSample()
  }
#endif
