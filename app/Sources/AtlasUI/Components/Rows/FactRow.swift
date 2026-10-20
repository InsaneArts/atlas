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
    .font(Typography.font(textStyle))
    .padding(.horizontal, inset)
    .frame(height: height)
    .accessibilityElement(children: .combine)
  }

  /// The row's text size.
  private var textStyle: TextStyle {
    switch size {
    case .compact: .callout
    case .regular: .body
    }
  }

  /// The space on each side of the row.
  private var inset: CGFloat {
    switch size {
    case .compact: Spacing.l
    case .regular: Spacing.xl
    }
  }

  /// The row's height.
  private var height: CGFloat {
    switch size {
    case .compact: ControlHeight.compactRow
    case .regular: ControlHeight.large
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.fact)
  }
#endif
