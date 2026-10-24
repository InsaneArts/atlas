import SwiftUI

/// In the helper: whether to let the account that asked use this one through Atlas, under the pairing of that
/// account's avatar handing over to Atlas at one size (v3 08). Allow is the footer's.
struct AllowScreen: View {
  /// Who asked, and what allowing means.
  let state: OnboardingAllowScreenState
  /// The status under what allowing means while Allow runs or failed.
  let note: WorkNote

  /// The pairing, what allowing means, the status and how to undo it.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(
        label: state.label,
        title: state.title,
        tail: OnboardingScreen.allowTail,
        lede: state.lede
      ) {
        AllowPairing(initial: state.initial)
      }
    } bottom: {
      if let status = StatusLine(note: note) {
        status
          .padding(.top, OnboardingLayout.Rhythm.content)
          .pagePiece(.status)
      }
      Note(text: "Atlas can be removed from both accounts at any time.")
        .padding(.top, OnboardingLayout.Rhythm.status)
        .pagePiece(.status)
    }
  }
}

#if DEBUG
  #Preview("Helper allow") {
    AllowScreen(state: PreviewStates.onboardingAllowScreen, note: .none)
      .onboardingSample()
  }
#endif
