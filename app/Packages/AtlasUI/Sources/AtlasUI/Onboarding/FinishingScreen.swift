import SwiftUI

/// The end-to-end check, once the person is back from the helper. Continue, or Try Again once it failed, is the
/// footer's.
struct FinishingScreen: View {
  /// The checklist and how the check stands.
  let state: OnboardingFinishingScreenState

  /// The checklist, then why it failed, or what the last check does.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(label: "Welcome back", title: state.title, tail: state.tail)
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
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      .pagePiece(.content)
      outcome
        .pagePiece(.status)
    }
  }

  /// Why the check failed, as the status under the checklist, or what the last check does, as a note.
  @ViewBuilder private var outcome: some View {
    if let failure = state.failure {
      Note(text: failure, tone: .attention)
        .padding(.top, OnboardingLayout.Rhythm.content)
    } else {
      Note(text: "The last check starts input in the helper, takes a screenshot and confirms it.")
        .padding(.top, OnboardingLayout.Rhythm.status)
    }
  }
}

#if DEBUG
  #Preview("Finishing") {
    FinishingScreen(state: PreviewStates.onboardingFinishingScreen)
      .onboardingSample()
  }

  #Preview("Finishing failed") {
    FinishingScreen(state: PreviewStates.onboardingFinishingFailedScreen)
      .onboardingSample()
  }
#endif
