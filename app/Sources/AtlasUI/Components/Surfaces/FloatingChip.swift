import SwiftUI

/// A status capsule floating over the helper's desktop, as the onboarding's pills look.
struct FloatingChip<Content: View>: View {
  /// What the chip says: a few texts and marks, laid in a row.
  @ViewBuilder let content: Content

  var body: some View {
    HStack(spacing: Spacing.m) {
      content
    }
    .font(Typography.font(.label))
    .lineLimit(1)
    .padding(.horizontal, Spacing.xl)
    .frame(height: ControlHeight.compactRow)
    .background(
      Capsule()
        .fill(.ultraThinMaterial)
    )
    .background(
      Capsule()
        .fill(Color.fillChip)
    )
    .overlay(
      Capsule()
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
    )
    .environment(\.colorScheme, .dark)
    .accessibilityElement(children: .combine)
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(spacing: Spacing.m) {
      FloatingChip {
        Text(verbatim: "Control released.")
      }
      FloatingChip {
        StatusDot(tone: .active)
        Text(verbatim: "You have control")
      }
    }
    .sampleSurface()
  }
#endif
