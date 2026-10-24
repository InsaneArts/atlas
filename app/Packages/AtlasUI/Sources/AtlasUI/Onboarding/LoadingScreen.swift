import SwiftUI

/// What the window shows before the first reading: a spinner while it runs, held back a moment so a quick reading
/// never flashes it (v3 S9), or why it failed, under the problem's hero (v3 D5). Try Again is the footer's.
struct LoadingScreen: View {
  /// Why the first reading failed; nil while it runs.
  let problem: String?

  /// The spinner, or the problem under its hero and title.
  var body: some View {
    Group {
      if let problem {
        ScrollColumn(width: OnboardingLayout.column) {
          OnboardingHeader(title: OnboardingScreen.loadingFailedTitle) {
            HeroDisc(kind: .problem)
          }
          StatusLine(kind: .problem, text: problem)
            .padding(.top, OnboardingLayout.Rhythm.content)
            .pagePiece(.status)
        }
      } else {
        Spinner(size: IconSize.sPlus)
          .appearsAfterDelay()
          // The spinner says nothing to VoiceOver, so the screen says what it waits for.
          .accessibilityElement(children: .ignore)
          .accessibilityLabel("Loading")
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .padding(.top, OnboardingLayout.contentTop)
  }
}

#if DEBUG
  #Preview("Loading") {
    LoadingScreen(problem: nil)
      .onboardingSample()
  }

  #Preview("Loading failed") {
    LoadingScreen(problem: PreviewStates.onboardingLoadingProblem)
      .onboardingSample()
  }
#endif
