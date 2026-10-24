import SwiftUI

/// In the helper: whether to let the account that asked use this one through Atlas, under the pairing of that
/// account's picture handing over to Atlas at one size. Allow and Not Now are the footer's.
struct AllowScreen: View {
  /// Who asked, and what allowing means.
  let state: OnboardingAllowScreenState
  /// The status under what allowing means while Allow runs or failed.
  let note: WorkNote

  /// The pairing, what allowing means, the status and how to undo it.
  var body: some View {
    ScreenColumn {
      OnboardingHeader(
        label: state.label,
        title: state.title,
        tail: OnboardingScreen.allowTail,
        lede: state.lede
      ) {
        AllowPairing(initial: state.initial, picture: state.picture)
      }
      StatusSlot(note: note)
      Note(text: "Atlas can be removed from both accounts at any time.")
        .padding(.top, OnboardingLayout.Rhythm.status)
    }
  }
}

#if DEBUG
  #Preview("Helper allow") {
    AllowScreen(state: PreviewStates.onboardingAllowScreen, note: .none)
      .onboardingSample()
  }
#endif
