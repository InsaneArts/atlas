import SwiftUI

/// A section's name over its card on a page of settings, in title case and the body's semibold, as Settings' pages
/// head their groups: "Ready to Act".
struct SectionTitle: View {
  /// The section's name.
  let text: String

  /// The name, read as a header.
  var body: some View {
    Text(text)
      .textStyle(.body, weight: .semibold)
      .foregroundStyle(Color.textPrimary)
      .accessibilityAddTraits(.isHeader)
  }
}
