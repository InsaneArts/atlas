import SwiftUI

/// The bar over a Settings page on the content card: the page's title within the page's own edges, level with the
/// window buttons. The title swaps in place by opacity, with the page.
struct SettingsTopBar: View {
  /// The route's title.
  let title: String

  /// The title.
  var body: some View {
    HStack(spacing: .zero) {
      Text(verbatim: title)
        .textStyle(.heading)
        .foregroundStyle(Color.textPrimary)
        .lineLimit(1)
        .truncationMode(.middle)
        .help(Text(verbatim: title))
        .swapsText(on: title)
        .accessibilityAddTraits(.isHeader)
      Spacer(minLength: .zero)
    }
    .padding(.horizontal, SettingsLayout.barInset)
    .frame(height: SettingsLayout.barHeight)
  }
}
