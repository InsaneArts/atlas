import SwiftUI

/// A group's name above its card or its rows, in sentence case: "Ready to act".
struct GroupLabel: View {
  /// The group's name.
  let text: String

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
