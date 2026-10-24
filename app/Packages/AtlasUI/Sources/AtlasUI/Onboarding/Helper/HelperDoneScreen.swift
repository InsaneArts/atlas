import SwiftUI

/// In the helper: set up, so the person switches back to their own account, under the success hero whose check draws
/// itself once. Switch Back is the footer's.
struct HelperDoneScreen: View {
  /// Who to switch back to.
  let state: OnboardingHelperDoneScreenState
  /// The status under what happens from now on while Switch Back runs or failed.
  let note: WorkNote

  /// The success hero, what happens from now on, the status and what Switch Back does.
  var body: some View {
    ScreenColumn {
      OnboardingHeader(
        title: OnboardingScreen.helperDoneTitle,
        tail: state.tail,
        lede:
          "You won’t need to set anything up here again. Atlas starts in this account by itself whenever it is logged in, and finishes setting up in your own account."
      ) {
        HeroDisc(kind: .success)
      }
      StatusSlot(note: note)
      Note(text: "Locks this account and opens the lock screen.")
        .padding(.top, OnboardingLayout.Rhythm.status)
    }
  }
}

#if DEBUG
  #Preview("Helper done") {
    HelperDoneScreen(state: PreviewStates.onboardingHelperDoneScreen, note: .none)
      .onboardingSample()
  }
#endif
