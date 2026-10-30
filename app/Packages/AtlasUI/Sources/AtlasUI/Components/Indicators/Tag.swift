import SwiftUI

/// A short word on a soft capsule beside a name, such as an app's "In front".
struct Tag: View {
  /// 20: the capsule's height, which a row of tags keeps even while it holds none.
  static let height: CGFloat = 20

  /// How the tag is coloured.
  enum Tone: Sendable, CaseIterable {
    /// A control's fill under a secondary word: a plain fact.
    case quiet
    /// A blue tint under a blue word: what is happening now, such as "In front".
    case accent
    /// A soft orange under an orange word: something to notice.
    case attention
  }

  /// The word.
  let text: String
  /// How it is coloured.
  var tone = Tone.quiet

  /// The word on its capsule.
  var body: some View {
    Text(verbatim: text)
      .font(Typography.font(.caption, weight: .semibold))
      .foregroundStyle(ink)
      .lineLimit(1)
      .padding(.horizontal, Spacing.s)
      .frame(height: Self.height)
      .background(
        Capsule()
          .fill(fill)
      )
  }

  /// The word's paint.
  private var ink: Color {
    switch tone {
    case .quiet: .textSecondary
    case .accent: .textLink
    case .attention: .textAttention
    }
  }

  /// The capsule's paint.
  private var fill: Color {
    switch tone {
    case .quiet: .fillControl
    case .accent:
      .accent
        .opacity(Opacity.tint)
    case .attention: .fillAttentionSoft
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    IndicatorSamples(.tag)
  }
#endif
