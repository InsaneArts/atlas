import SwiftUI

/// One account in the Accounts pane: its first letter on a disc, its name over when it was added, and Remove….
struct AccountRow: View {
  /// The account.
  let row: SettingsAccountRow
  /// Whether Remove… takes presses now.
  let isEnabled: Bool
  /// Asks to take it off the list.
  let remove: @MainActor () -> Void

  /// What it draws, from its state.
  var body: some View {
    ListRowBody(row.title, detail: row.detail, lead: .letter(row.letter), size: .compact) {
      Button("Remove…", action: remove)
        .buttonStyle(.capsule(.secondary, size: .mini))
        .disabled(!isEnabled)
        .help("Take \(row.title) off the list")
    }
  }
}
