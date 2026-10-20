import SwiftUI

/// The "Live" capsule over a live picture.
struct LiveBadge: View {
  var body: some View {
    HStack(spacing: Layout.gap) {
      StatusDot(tone: .ready, size: .live)
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

/// The badge's own measures.
private enum Layout {
  /// [R] 5: between the dot and the word, kept exact until Phase 3b rounds it to `Spacing.xs` (4).
  static let gap: CGFloat = 5
}

#if DEBUG
  #Preview("Samples") {
    LiveBadge()
      .sampleSurface()
  }
#endif
