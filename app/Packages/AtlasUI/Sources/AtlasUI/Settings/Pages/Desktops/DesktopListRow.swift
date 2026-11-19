import SwiftUI

/// One desktop in the Desktops list, the whole row a button on a card row's inset fill (``CardRowStyle``) that opens
/// its page, as an account's row does: its kind's symbol on a tile, its title with a quiet "Default" on the default
/// one, what it is with how it stands, and a chevron.
struct DesktopListRow: View {
  /// The desktop.
  let row: SettingsDesktopRow
  /// Opens its page.
  let open: @MainActor () -> Void

  /// The row as a button.
  var body: some View {
    Button(action: open) {
      ListRowBody(
        row.title,
        detail: row.detail,
        tag: row.isDefault ? "Default" : nil,
        tagTone: .quiet,
        titleFit: .oneLine,
        lead: .tile(.symbol(row.isVM ? SymbolName.window.rawValue : SymbolName.personCircle.rawValue)),
        size: .compact
      ) {
        Image(systemName: SymbolName.next.rawValue)
          .font(Typography.font(.callout, weight: .semibold))
          .foregroundStyle(Color.textTertiary)
          .accessibilityHidden(true)
      }
    }
    .buttonStyle(.cardRow)
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(.isButton)
    .accessibilityHint(Text(verbatim: "Opens its page"))
  }
}
