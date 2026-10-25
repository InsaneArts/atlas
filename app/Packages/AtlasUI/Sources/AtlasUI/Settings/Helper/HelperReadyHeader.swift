import SwiftUI

/// The "Ready to act" section's name, with Check Again at its trailing edge as a link, since it checks every row
/// below, not one. The link keeps its look and width while a check runs: the rows' marks say it is checking, and a
/// press meanwhile does nothing.
struct HelperReadyHeader: View {
  /// Runs Check Again.
  let checkAgain: @MainActor () -> Void

  /// The section's name and the link.
  var body: some View {
    HStack(alignment: .firstTextBaseline) {
      GroupLabel(text: "Ready to act")
      Spacer(minLength: Spacing.s)
      Button("Check Again", action: checkAgain)
        .buttonStyle(.textLink(.small))
        .help("Check the helper again now")
    }
    .padding(.horizontal, Spacing.m)
  }
}
