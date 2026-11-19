import SwiftUI

/// One row of a VM's page: its title over what it means, and at its end its value in the secondary colour, or its
/// small button, Remove… in the destructive tint, dimmed while it cannot run.
struct VMRowView: View {
  /// The row.
  let row: SettingsVMRow
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    ListRowBody(row.title, detail: row.detail, size: .compact) {
      if let value = row.value {
        Text(value)
          .textStyle(.body)
          .foregroundStyle(Color.textSecondary)
          .lineLimit(1)
          .monospacedDigit()
      }
      if let button = row.button {
        Button(button.title) { send(button.event) }
          .buttonStyle(.capsule(button.isDestructive ? .destructive : .secondary, size: .row))
          .disabled(!button.isEnabled)
      }
    }
  }
}
