import SwiftUI

/// In the helper: set up, so the person switches back to their own account.
struct HelperDoneScreen: View {
  /// Who to switch back to.
  let state: OnboardingHelperDoneScreenState
  /// The line under Switch Back.
  let note: WorkNote
  /// Whether a command runs.
  let isBusy: Bool
  /// Receives Switch Back.
  let send: @MainActor (OnboardingEvent) -> Void

  /// A check, what happens from now on, and Switch Back.
  var body: some View {
    ScreenColumn(width: OnboardingLayout.helperDoneColumn) {
      CheckMark(size: IconSize.hero)
      Headline(title: OnboardingScreen.helperDoneTitle, tail: state.tail)
        .padding(.top, Spacing.s)
      Lede(
        text:
          "You won't need to set anything up here again. Atlas starts in this account by itself whenever it is logged in, and finishes setting up in your own account."
      )
    } bottom: {
      Button("Switch Back") { send(.switchBack) }
        .buttonStyle(.capsule(size: .large))
        .keyboardShortcut(.defaultAction)
        .disabled(isBusy)
        .padding(.top, Spacing.ml)
      WorkNoteLine(note: note)
      Note(text: "Locks this account and opens the lock screen.")
    }
  }
}

#if DEBUG
  #Preview("Helper done") {
    HelperDoneScreen(state: PreviewStates.onboardingHelperDoneScreen, note: .none, isBusy: false) { _ in }
      .onboardingSample()
  }
#endif
