import SwiftUI

/// A group's name above its card or its rows, in sentence case: "Ready to act".
struct GroupLabel: View {
  /// The group's name.
  let text: String

  /// The label in small semibold, read as a header.
  var body: some View {
    Text(text)
      .font(Typography.font(.caption, weight: .semibold))
      .foregroundStyle(Color.textTertiary)
      .accessibilityAddTraits(.isHeader)
  }
}

#if DEBUG
  #Preview("Samples") {
    GroupLabelSamples()
  }
#endif
