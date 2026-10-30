import SwiftUI

/// A status as a small pill: a dot and the word in the status's colour on a faint wash of it, or on a quiet
/// fill when there is nothing to say.
struct StatusPill: View {
  /// The status in words, such as "Idle".
  let label: String
  /// How it reads at a glance.
  let tone: StatusTone

  /// What it draws, from its state.
  var body: some View {
    HStack(spacing: StatusPillLayout.gap) {
      Circle()
        .fill(Self.foreground(tone))
        .frame(width: IconSize.xxs, height: IconSize.xxs)
      Text(label)
        .font(Typography.font(.caption, weight: .semibold))
        .foregroundStyle(Self.foreground(tone))
        .lineLimit(1)
    }
    .padding(.horizontal, Spacing.s)
    .frame(height: StatusPillLayout.height)
    .background(
      Capsule()
        .fill(Self.background(tone))
    )
    .fixedSize()
    .accessibilityElement(children: .ignore)
    .accessibilityLabel(label)
  }

  /// The dot's and the word's colour.
  ///
  /// - Parameter tone: How the status reads.
  /// - Returns: Green when ready, orange when the person is needed, the link colour while working or in control,
  ///   secondary otherwise.
  nonisolated static func foreground(_ tone: StatusTone) -> Color {
    switch tone {
    case .ready: .textSuccess
    case .attention: .textAttention
    case .active, .link: .textLink
    case .quiet: .textSecondary
    }
  }

  /// The pill's fill: a faint wash of the status's colour, or the quiet control fill.
  ///
  /// - Parameter tone: How the status reads.
  /// - Returns: The fill.
  nonisolated static func background(_ tone: StatusTone) -> Color {
    switch tone {
    case .ready:
      Color.success
        .opacity(Opacity.tint)
    case .attention:
      Color.attention
        .opacity(Opacity.tint)
    case .active, .link:
      Color.accent
        .opacity(Opacity.tint)
    case .quiet: .fillControl
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    IndicatorSamples(.statusPill)
  }
#endif
