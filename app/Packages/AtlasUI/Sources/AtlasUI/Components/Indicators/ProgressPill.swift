import SwiftUI

/// A status line in a pill: a spinner while waiting, a check when done.
struct ProgressPill: View {
  /// What is awaited, or what is done.
  let text: String
  /// Whether it is done.
  var done = false

  var body: some View {
    HStack(spacing: Spacing.s) {
      if done {
        CheckMark(size: IconSize.xs)
      } else {
        Spinner(size: IconSize.xs)
      }
      Text(text)
        .font(Typography.font(.callout))
        .foregroundStyle(done ? Color.textSuccess : Color.textSecondary)
    }
    .padding(.horizontal, Spacing.mPlus)
    .frame(minHeight: ControlHeight.sPlus)
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
