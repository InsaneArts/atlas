import SwiftUI

/// The end-to-end check, once the person is back from the helper.
struct FinishingScreen: View {
  /// The checklist and how the check stands.
  let state: OnboardingFinishingScreenState
  /// Receives Try Again.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The checklist, then why it failed with Try Again, or what the last check does.
  var body: some View {
    ScreenColumn {
      GroupLabel(text: "Welcome back")
      Headline(title: state.title, tail: state.tail)
      CardGroup {
        CheckRow(text: state.loggedIn, state: .done, note: "in the background")
          .lineLimit(OnboardingLayout.nameLines)
          .truncationMode(.middle)
        SeparatorLine()
        CheckRow(text: "Atlas runs there", state: .done, note: "starts by itself")
        SeparatorLine()
        CheckRow(text: "Screen Recording and Accessibility", state: .done, note: "allowed")
        SeparatorLine()
        CheckRow(text: "Mouse, keyboard and a test screenshot", state: state.live, note: state.liveNote)
      }
      .padding(.top, Spacing.s)
      if let failure = state.failure {
        Note(text: failure, tone: .attention)
        Button("Try Again") { send(.retryFinish) }
          .buttonStyle(.capsule(.glass))
          .keyboardShortcut(.defaultAction)
      } else {
        Note(text: "The last check starts input in the helper, takes a screenshot and confirms it.")
      }
    }
  }
}

#if DEBUG
  #Preview("Finishing") {
    FinishingScreen(state: PreviewStates.onboardingFinishingScreen) { _ in }
      .onboardingSample()
  }

  #Preview("Finishing failed") {
    FinishingScreen(state: PreviewStates.onboardingFinishingFailedScreen) { _ in }
      .onboardingSample()
  }
#endif
