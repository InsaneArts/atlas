import SwiftUI

/// The bar over a Settings page on the content card: the page's title within the page's own edges, level with the
/// window buttons, and on an account's page Back before it, to Accounts, as System Settings' own pages pushed over a
/// list go back. The title swaps in place by opacity, with the page.
struct SettingsTopBar: View {
  /// The route's title.
  let title: String
  /// The page Back goes to and its title; nil on a page of its own, which has no Back.
  var back: (page: SettingsPage, title: String)?
  /// Receives Back.
  let send: @MainActor (SettingsEvent) -> Void

  /// Back, if any, and the title.
  var body: some View {
    HStack(spacing: SettingsLayout.backGap) {
      if let back {
        Button {
          send(.select(back.page))
        } label: {
          Image(systemName: SymbolName.back.rawValue)
        }
        .buttonStyle(.capsule(.glyph, size: .compact, outline: .circle))
        .keyboardShortcut("[", modifiers: .command)
        .help(Text(verbatim: "Back to \(back.title)"))
        .accessibilityLabel(Text(verbatim: "Back to \(back.title)"))
      }
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
    .padding(.leading, back == nil ? SettingsLayout.barInset : SettingsLayout.backInset)
    .padding(.trailing, SettingsLayout.barInset)
    .frame(height: SettingsLayout.barHeight)
  }
}
