import SwiftUI

/// What the window shows before the first reading: a spinner while it runs, or why it failed with Try Again.
struct LoadingScreen: View {
  /// Why the first reading failed; nil while it runs.
  let problem: String?
  /// Receives Try Again.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The spinner, or the problem over Try Again.
  var body: some View {
    if let problem {
      ScreenColumn {
        Headline(title: OnboardingScreen.loadingFailedTitle, size: .compact)
        Note(text: problem, tone: .attention)
        Button("Try Again") { send(.checkAgain) }
          .buttonStyle(.capsule(size: .large))
          .keyboardShortcut(.defaultAction)
          .padding(.top, Spacing.m)
      }
    } else {
      Spinner(size: IconSize.mark)
        // The spinner says nothing to VoiceOver, so the screen says what it waits for.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Loading")
    }
  }
}

#if DEBUG
  #Preview("Loading") {
    LoadingScreen(problem: nil) { _ in }
      .sampleSurface()
  }

  #Preview("Loading failed") {
    LoadingScreen(problem: PreviewStates.onboardingLoadingProblem) { _ in }
      .onboardingSample()
  }
#endif
