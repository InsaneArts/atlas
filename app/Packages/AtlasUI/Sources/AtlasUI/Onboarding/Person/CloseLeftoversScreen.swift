import SwiftUI

/// A deleted account's login is still open, which can stall the next login: the problem's hero and what closing it
/// does, then the success hero once it is closed. Close It… and Continue are the footer's.
struct CloseLeftoversScreen: View {
  /// What the screen says.
  let state: OnboardingCloseScreenState
  /// The status while Atlas closes it, or why it did not.
  let note: WorkNote

  /// The hero, the headline and what it does, then the status in its kept place.
  var body: some View {
    ScreenColumn {
      OnboardingHeader(title: state.title, lede: state.lede, swapsTitle: true) {
        HeroDisc(kind: state.isClosed ? .success : .problem)
      }
      StatusSlot(note: note)
    }
  }
}

#if DEBUG
  #Preview("Close") {
    CloseLeftoversScreen(state: PreviewStates.onboardingCloseScreen, note: .none)
      .onboardingSample()
  }

  #Preview("Closed") {
    CloseLeftoversScreen(state: PreviewStates.onboardingClosedScreen, note: .none)
      .onboardingSample()
  }
#endif
