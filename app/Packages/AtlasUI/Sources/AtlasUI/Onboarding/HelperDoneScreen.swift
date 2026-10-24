import SwiftUI

/// In the helper: set up, so the person switches back to their own account. Switch Back is the footer's.
struct HelperDoneScreen: View {
  /// Who to switch back to.
  let state: OnboardingHelperDoneScreenState
  /// The line under what happens from now on.
  let note: WorkNote

  /// A check, what happens from now on, the work note and what Switch Back does.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(
        title: OnboardingScreen.helperDoneTitle,
        tail: state.tail,
        lede:
          "You won't need to set anything up here again. Atlas starts in this account by itself whenever it is logged in, and finishes setting up in your own account."
      ) {
        CheckMark(size: IconSize.xxl)
      }
    } bottom: {
      WorkNoteLine(note: note)
        .padding(.top, OnboardingLayout.Rhythm.content)
        .pagePiece(.status)
      Note(text: "Locks this account and opens the lock screen.")
        .padding(.top, OnboardingLayout.Rhythm.status)
        .pagePiece(.status)
    }
  }
}

#if DEBUG
  #Preview("Helper done") {
    HelperDoneScreen(state: PreviewStates.onboardingHelperDoneScreen, note: .none)
      .onboardingSample()
  }
#endif
