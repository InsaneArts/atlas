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
        .font(OnboardingLayout.blockedSymbolFont)
        .foregroundStyle(Color.attention)
        .accessibilityHidden(true)
      Headline(title: OnboardingScreen.blockedTitle)
      Lede(text: reason)
      Button("Check Again") { send(.checkAgain) }
        .buttonStyle(.capsule(size: .large))
        .keyboardShortcut(.defaultAction)
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
