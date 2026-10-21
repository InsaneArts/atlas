import SwiftUI

/// A status capsule floating over the helper's desktop, as the onboarding's pills look.
struct FloatingChip<Content: View>: View {
  /// The most lines its text takes: one for a status, a few for a notice that must be read whole.
  var lines = 1
  /// What the chip says: a few texts and marks, laid in a row.
  @ViewBuilder let content: Content

  var body: some View {
    HStack(spacing: Spacing.m) {
      content
    }
    .font(Typography.font(.callout))
    .lineLimit(lines)
    .multilineTextAlignment(.center)
    .padding(.horizontal, Spacing.xl)
    .padding(.vertical, Spacing.s)
    .frame(minHeight: ControlHeight.compactRow)
    .background(
      shape
        .fill(.ultraThinMaterial)
    )
    .background(
      shape
        .fill(Color.fillChip)
    )
    .overlay(
      shape
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
    )
    .environment(\.colorScheme, .dark)
    .accessibilityElement(children: .combine)
  }

  /// A capsule while the chip is one line high; its ends keep that roundness as more lines make it taller.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: ControlHeight.compactRow / 2, style: .circular)
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
