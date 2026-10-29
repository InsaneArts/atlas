import SwiftUI

/// A group's quiet heading in one of the card's lists, scrolling with its rows, as every list's headings do: its
/// label and, where the list counts it, how many it holds, the count rolling as it changes. The list gives its room.
struct ListHeading: View {
  /// The group's label, such as "Now" or "Opened by agents".
  let title: String
  /// How many the group holds; nil draws none.
  var count: Int?

  /// The label and its count, read as one header.
  var body: some View {
    HStack(spacing: Spacing.xs) {
      Text(verbatim: title)
        .font(Typography.font(.callout, weight: .semibold))
      if let count {
        Text(verbatim: "· \(count)")
          .font(Typography.digits(.callout, weight: .medium))
          .contentTransition(.numericText())
      }
    }
    .foregroundStyle(Color.textTertiary)
    .lineLimit(1)
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(.isHeader)
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(alignment: .leading, spacing: Spacing.m) {
      ListHeading(title: String("Now"))
      ListHeading(title: String("Opened by agents"), count: 4)
    }
    .padding(Spacing.l)
    .sampleSurface()
  }
#endif
