import SwiftUI

/// One account in the Accounts pane: its first letter on a disc, its name over when it was added, and how freely
/// agents act there, as three segments beside Remove…, or, where they don't fit, a popup that holds Remove from
/// List… too.
struct AccountRow: View {
  /// The account.
  let row: SettingsAccountRow
  /// Whether its controls take presses now.
  let isEnabled: Bool
  /// Receives the level the person picks.
  let pick: @MainActor (AccountLevelChoice) -> Void
  /// Asks to take it off the list.
  let remove: @MainActor () -> Void

  /// What it draws, from its state.
  var body: some View {
    ListRowBody(row.title, detail: row.detail, lead: .letter(row.letter), size: .compact) {
      ViewThatFits(in: .horizontal) {
        HStack(spacing: Spacing.s) {
          AccountLevelControl(account: row.title, level: row.level, pick: pick)
            .disabled(!isEnabled)
          Button("Remove…", action: remove)
            .buttonStyle(.capsule(.secondary, size: .mini))
            .disabled(!isEnabled)
            .help("Take \(row.title) off the list")
        }
        AccountLevelMenu(account: row.title, level: row.level, isEnabled: isEnabled, pick: pick, remove: remove)
      }
      // Offered the row's room before the title, which cuts itself short, so the segments show wherever they fit.
      .layoutPriority(1)
    }
  }
}
