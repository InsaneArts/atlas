import SwiftUI

/// One account in the Accounts pane: its first letter on a disc, its name over when it was added, and how freely
/// agents act there, as three segments beside a ⋯ menu of what else can be done with it, or, where they don't fit, a
/// popup that holds the levels and the same items.
struct AccountRow: View {
  /// The account.
  let row: SettingsAccountRow
  /// Whether its controls take presses now.
  let isEnabled: Bool
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    ListRowBody(row.title, detail: row.detail, lead: .letter(row.letter), size: .compact) {
      ViewThatFits(in: .horizontal) {
        HStack(spacing: Spacing.s) {
          AccountLevelControl(account: row.title, level: row.level, pick: pick)
          Menu {
            AccountActions(row: row, send: send)
          } label: {
            Image(systemName: SymbolName.more.rawValue)
          }
          .menuStyle(.button)
          .menuIndicator(.hidden)
          .buttonStyle(.capsule(.secondary, size: .mini, outline: .circle))
          .fixedSize()
          .help("More for \(row.title)")
          .accessibilityLabel("More for \(row.title)")
        }
        .disabled(!isEnabled)
        AccountLevelMenu(account: row.title, level: row.level, isEnabled: isEnabled, pick: pick) {
          AccountActions(row: row, send: send)
        }
      }
      // Offered the row's room before the title, which cuts itself short, so the segments show wherever they fit.
      .layoutPriority(1)
    }
  }

  /// Sets the level the person picked.
  ///
  /// - Parameter level: The level.
  private func pick(_ level: AccountLevelChoice) {
    send(.setAccountLevel(row.id, level))
  }
}
