import SwiftUI

/// What the window shows before the first reading: a spinner while it runs, held back a moment so a quick reading
/// never flashes it, or why it failed, under the problem's hero. Try Again is the footer's.
struct LoadingScreen: View {
  /// Why the first reading failed; nil while it runs.
  let problem: String?

  /// The spinner, or the problem under its hero and title.
  var body: some View {
    Group {
      if let problem {
        ScreenColumn {
          OnboardingHeader(title: OnboardingScreen.loadingFailedTitle) {
            HeroDisc(kind: .problem)
          }
          StatusSlot(line: StatusLine(kind: .problem, text: problem))
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
