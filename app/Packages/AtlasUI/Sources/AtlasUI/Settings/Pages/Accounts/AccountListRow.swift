import SwiftUI

/// One account in the Accounts list, the whole row a button on a card row's inset fill (``CardRowStyle``) that opens
/// its page: its first letter on a disc, its name over its level and when it was added, and a chevron.
struct AccountListRow: View {
  /// The account.
  let row: SettingsAccountRow
  /// Opens its page.
  let open: @MainActor () -> Void

  /// The row as a button.
  var body: some View {
    Button(action: open) {
      ListRowBody(row.title, detail: row.detail, titleFit: .oneLine, lead: .letter(row.letter), size: .compact) {
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
