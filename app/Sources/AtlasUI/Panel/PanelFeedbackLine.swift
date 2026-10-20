import SwiftUI

/// What the last action said, with a spinner while it still runs.
struct PanelFeedbackLine: View {
  /// The words, and whether the action still runs.
  let feedback: Feedback

  /// The spinner, then the words.
  var body: some View {
    HStack(spacing: Spacing.m) {
      if feedback.isWorking {
        Spinner(size: IconSize.glyph)
      }
      Text(feedback.text)
        .font(Typography.font(.caption))
        .foregroundStyle(Color.textSecondary)
        .fixedSize(horizontal: false, vertical: true)
      Spacer(minLength: .zero)
    }
    .accessibilityElement(children: .combine)
  }
}

#if DEBUG
  #Preview("Working") {
    PanelFeedbackLine(feedback: Feedback(text: String("Logging out the helper…"), isWorking: true))
      .frame(width: SampleWidth.narrow)
      .sampleSurface()
  }
#endif
