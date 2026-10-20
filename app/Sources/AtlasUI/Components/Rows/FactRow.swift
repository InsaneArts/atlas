import SwiftUI

/// A name and its value on one line, the value on the right.
struct FactRow: View {
  /// How large the row is.
  enum Size: Sendable {
    /// The panel's narrow card.
    case compact
    /// The sidebar's card.
    case regular
  }

  /// What the fact is.
  let name: String
  /// Its value, cut short with its whole value as a tooltip.
  let value: String
  /// How large the row is.
  var size = Size.compact

  var body: some View {
    HStack(spacing: Spacing.l) {
      Text(name)
        .foregroundStyle(Color.textSecondary)
      Spacer(minLength: Spacing.m)
      Text(value)
        .lineLimit(1)
        .truncationMode(.tail)
        .help(value)
    }
    .font(Typography.font(size == .compact ? .callout : .body))
    .padding(.horizontal, size == .compact ? Spacing.l : Spacing.xl)
    .frame(height: size == .compact ? ControlHeight.compactRow : ControlHeight.large)
    .accessibilityElement(children: .combine)
  }
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.fact)
  }
#endif
