import SwiftUI

/// The Settings heading treatment for sections without a header description.
struct SettingsSectionHeading: View {
  /// The section's name.
  let title: String

  /// Aligns the heading with the existing row labels.
  var body: some View {
    Text(title)
      .textStyle(.heading)
      .foregroundStyle(Color.textPrimary)
      .padding(.horizontal, Spacing.m)
      .accessibilityAddTraits(.isHeader)
  }
}
