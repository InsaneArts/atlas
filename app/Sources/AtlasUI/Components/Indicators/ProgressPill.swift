import SwiftUI

/// A status line in a pill: a spinner while waiting, a check when done.
struct ProgressPill: View {
  /// What is awaited, or what is done.
  let text: String
  /// Whether it is done.
  var done = false

  var body: some View {
    HStack(spacing: Spacing.m) {
      if done {
        CheckMark(size: IconSize.glyph)
      } else {
        Spinner(size: IconSize.glyph)
      }
      Text(text)
        .font(Typography.font(.callout))
        .foregroundStyle(done ? Color.textSuccess : Color.textSecondary)
    }
    .padding(.horizontal, Spacing.xl)
    .frame(minHeight: ControlHeight.row)
    .background(
      Capsule()
        .fill(Color.fillPill)
    )
    .accessibilityElement(children: .combine)
  }
}

#if DEBUG
  #Preview("Samples") {
    IndicatorSamples(.progressPill)
  }
#endif
