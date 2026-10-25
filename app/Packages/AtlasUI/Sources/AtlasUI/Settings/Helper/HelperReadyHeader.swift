import SwiftUI

/// The "Ready to act" section's name, with Check Again at its trailing edge as a link, since it checks every row
/// below, not one. The link keeps its look and width while a check runs: the rows' marks say it is checking, and a
/// press meanwhile does nothing.
struct HelperReadyHeader: View {
  /// Runs Check Again.
  let checkAgain: @MainActor () -> Void

  /// The accessibility traits applied only to the section's title.
  let headingTraits: AccessibilityTraits = .isHeader

  /// The section's name and the link.
  var body: some View {
    HStack(alignment: .top) {
      VStack(alignment: .leading, spacing: Spacing.half) {
        Text("Ready to act")
          .textStyle(.heading)
          .foregroundStyle(Color.textPrimary)
          .accessibilityAddTraits(headingTraits)
        Text("Check the helper and the permissions it needs.")
          .textStyle(.callout)
          .foregroundStyle(Color.textSecondary)
      }
      Spacer(minLength: Spacing.s)
      Button("Check Again", action: checkAgain)
        .buttonStyle(.textLink(.small))
        .help("Check the helper again now")
    }
    .padding(.horizontal, Spacing.m)
  }
}
