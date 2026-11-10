import SwiftUI

/// The card at the top of the Helper page: Atlas's mark with the helper's dot, its name over what it is, and Switch to
/// Helper….
struct HelperStatusCard: View {
  /// The account.
  let account: SettingsHelperAccount
  /// Which buttons can run now, and Switch's tooltip.
  let buttons: SettingsHelperButtons
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    HStack(spacing: Spacing.m) {
      SidebarHelperMark(tone: account.status.tone, ground: .fillContent)
      VStack(alignment: .leading, spacing: Spacing.hairline) {
        Text(account.name)
          .textStyle(.body, weight: .semibold)
          .foregroundStyle(Color.textPrimary)
          .lineLimit(1)
        Text(account.line)
          .textStyle(.caption)
          .foregroundStyle(Color.textSecondary)
          .lineLimit(1)
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      .accessibilityElement(children: .combine)
      Button("Switch to Helper…") { send(.askSwitch) }
        .buttonStyle(.capsule(.secondary, size: .row))
        .disabled(!buttons.canSwitch)
        .help(buttons.switchHelp)
    }
    .padding(.horizontal, ListRow.Size.compact.inset)
    .frame(minHeight: SettingsLayout.headerRowHeight)
    .background(
      RoundedRectangle(cornerRadius: Radius.large, style: .continuous)
        .fill(Color.fillGroupInset)
    )
    .accessibilityElement(children: .contain)
  }
}
