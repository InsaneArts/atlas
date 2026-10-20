import SwiftUI

/// A status in a capsule: a dot in its tone's colour and its label.
struct StatusBadge: View {
  /// The status's name.
  let label: String
  /// How it reads at a glance.
  let tone: StatusTone

  var body: some View {
    HStack(spacing: Spacing.s) {
      StatusDot(tone: tone)
      Text(label)
        .font(Typography.font(.note, weight: .medium))
    }
    .padding(.horizontal, Spacing.ml)
    .frame(height: ControlHeight.pill)
    .background(
      Capsule()
        .fill(Color.fillPill)
    )
    .accessibilityElement(children: .combine)
  }
}

#if DEBUG
  #Preview("Samples") {
    IndicatorSamples(.statusBadge)
  }
#endif
