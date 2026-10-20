import SwiftUI

/// What the window shows before the first reading: a spinner, and why the reading failed if it did.
struct LoadingScreen: View {
  /// Why the first reading failed; nil while it runs.
  let problem: String?

  /// The spinner over any problem.
  var body: some View {
    VStack(spacing: Spacing.l) {
      Spinner(size: IconSize.mark)
      if let problem {
        Note(text: problem, tone: .attention)
      }
    }
    // The spinner says nothing to VoiceOver, so the screen says what it waits for.
    .accessibilityElement(children: .combine)
    .accessibilityLabel(problem ?? "Loading")
  }
}

#if DEBUG
  #Preview("Loading") {
    LoadingScreen(problem: nil)
      .sampleSurface()
  }
#endif
