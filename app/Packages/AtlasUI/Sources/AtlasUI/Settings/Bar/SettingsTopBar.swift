import SwiftUI

/// The bar over a Settings page on the content card: back and forward, then the page's title, level with the window
/// buttons. The title swaps in place by opacity, with the page.
struct SettingsTopBar: View {
  /// The route's title.
  let title: String
  /// Where the window stands, for Back and Forward.
  let navigation: SettingsNavigation
  /// Receives Back or Forward.
  let send: @MainActor (SettingsEvent) -> Void

  /// The capsule and the title.
  var body: some View {
    HStack(spacing: SettingsLayout.barGap) {
      NavCapsule(canGoBack: navigation.canGoBack, canGoForward: navigation.canGoForward, send: send)
      Text(verbatim: title)
        .textStyle(.heading)
        .foregroundStyle(Color.textPrimary)
        .lineLimit(1)
        .swapsText(on: title)
        .accessibilityAddTraits(.isHeader)
      Spacer(minLength: .zero)
    }
    .padding(.leading, SettingsLayout.barLeading)
    .frame(height: SettingsLayout.barHeight)
  }
}
