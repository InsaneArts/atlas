import SwiftUI

/// A capsule of buttons floating over a scrim, as a large screenshot's bar: frosted and lifted as a menu is, or drawn
/// opaque under Reduce Transparency.
struct FloatingBar<Content: View>: View {
  /// What the bar holds, laid in a row.
  @ViewBuilder let content: Content
  @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

  var body: some View {
    HStack(spacing: Spacing.ml) {
      content
    }
    .padding(Spacing.s)
    .background {
      if reduceTransparency {
        CapsuleEdge()
          .fill(Color.fillBarSolid)
      } else {
        CapsuleEdge()
          .fill(Color.fillBar)
          .background(.ultraThinMaterial, in: CapsuleEdge())
      }
    }
    .overlay(
      CapsuleEdge()
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
    )
    .elevation(.bar)
    .environment(\.colorScheme, .dark)
  }
}

#if DEBUG
  #Preview("Samples") {
    FloatingBar {
      Text(verbatim: "10:31:44 · 2560 × 1440")
        .font(Typography.font(.body, weight: .medium))
        .padding(.horizontal, Spacing.l)
      Button(String("Copy")) {}
        .buttonStyle(.capsule(.glass, size: .small))
      Button(String("Close")) {}
        .buttonStyle(.capsule(.primary, size: .small))
    }
    .padding(Spacing.page)
    .sampleSurface()
  }
#endif
