import SwiftUI

/// A frosted capsule floating over the helper's screen: a caption, the chip at the top, or the mini screen's Live.
/// Under Reduce Transparency it is drawn opaque on the window's ground.
struct FloatingChip<Content: View>: View {
  /// How tall the chip is.
  enum Size: Sendable {
    /// 24: a chip over a small picture, such as the mini screen's Live.
    case mini
    /// 30: a caption in the corner of the screen.
    case caption
    /// 32: the chip at the top of the screen, such as "You have control".
    case top
  }

  /// How tall the chip is while it is one line high.
  var size = Size.caption
  /// The most lines its text takes: one for a status, a few for a notice that must be read whole.
  var lines = 1
  /// What the chip says: a few texts and marks, laid in a row.
  @ViewBuilder let content: Content
  @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

  /// The content in a row on the frosted capsule.
  var body: some View {
    HStack(spacing: Spacing.s) {
      content
    }
    .font(Typography.font(.callout, weight: .medium))
    .lineLimit(lines)
    .multilineTextAlignment(.center)
    .padding(.horizontal, Spacing.m)
    .padding(.vertical, lines > 1 ? Spacing.xsPlus : .zero)
    .frame(minHeight: height)
    .background {
      if reduceTransparency {
        shape
          .fill(Color.ground)
      } else {
        shape
          .fill(Color.fillChip)
          .background(.ultraThinMaterial, in: shape)
      }
    }
    .overlay(
      shape
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
    )
    .environment(\.colorScheme, .dark)
    .accessibilityElement(children: .combine)
  }

  /// The chip's height while one line high.
  private var height: CGFloat {
    switch size {
    case .mini: ControlHeight.xs
    case .caption: ControlHeight.sPlus
    case .top: ControlHeight.m
    }
  }

  /// A capsule while the chip is one line high; its ends keep that roundness as more lines make it taller.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: height / 2, style: .circular)
  }
}

#if DEBUG
  #Preview("Samples") {
    FloatingChipSamples()
  }
#endif
