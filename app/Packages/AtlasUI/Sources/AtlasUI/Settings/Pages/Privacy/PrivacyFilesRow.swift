import SwiftUI

/// One row of the Privacy page: its title over its detail and its control at the end, at a Settings row's height and
/// insets. When `swap` changes, the words cross-fade in place over ``Motion/textSwap``, the row keeping its height.
struct PrivacyFilesRow<Trailing: View>: View {
  /// The title.
  let title: String
  /// The line under it.
  let detail: String
  /// What changes the words, which cross-fade when it does.
  let swap: Bool
  /// The control at the row's end.
  @ViewBuilder let trailing: Trailing

  /// The words and the control.
  var body: some View {
    ListRowBody(
      title,
      detailStyle: .plain,
      size: .compact,
      detailContent: { Text(detail) }
    ) {
      trailing
    }
    .swapsText(on: swap)
  }
}
