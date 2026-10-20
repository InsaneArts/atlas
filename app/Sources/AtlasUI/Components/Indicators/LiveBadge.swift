import SwiftUI

/// The "Live" capsule over a live picture.
struct LiveBadge: View {
  var body: some View {
    HStack(spacing: Spacing.xs) {
      StatusDot(tone: .ready, size: .small)
      Text("Live")
        .font(Typography.font(.caption2, weight: .semibold))
    }
    .padding(.horizontal, Spacing.m)
    .frame(height: ControlHeight.badge)
    .background(
      Capsule()
        .fill(Color.fillBadge)
    )
  }
}

#if DEBUG
  #Preview("Samples") {
    LiveBadge()
      .sampleSurface()
  }
#endif
