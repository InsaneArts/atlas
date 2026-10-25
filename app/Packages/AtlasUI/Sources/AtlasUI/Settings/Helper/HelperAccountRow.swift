import SwiftUI

/// The helper as an account, at the top of the Helper pane: its picture, its name over what it is,
/// its status as a pill, and Switch to the Helper….
struct HelperAccountRow: View {
  /// The account.
  let account: SettingsHelperAccount
  /// Which buttons can run now, and Switch's tooltip.
  let buttons: SettingsHelperButtons
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    HelperAccountLayout {
      HStack(spacing: Spacing.m) {
        AccountPicture(side: SettingsLayout.accountPicture)
        VStack(alignment: .leading, spacing: Spacing.half) {
          Text(account.name)
            .textStyle(.headline)
            .foregroundStyle(Color.textPrimary)
          Text(account.line)
            .textStyle(.callout)
            .foregroundStyle(Color.textSecondary)
        }
        .fixedSize(horizontal: false, vertical: true)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
      }
      HStack(spacing: Spacing.m) {
        StatusPill(label: account.status.label, tone: account.status.tone)
        Spacer(minLength: Spacing.m)
        SettingsAlertButton("Switch to the Helper…") { send(.askSwitch) }
          .buttonStyle(.capsule(.glass, size: .mini))
          .disabled(!buttons.canSwitch)
          .help(buttons.switchHelp)
      }
    }
    .padding(Spacing.m)
    .accessibilityElement(children: .contain)
  }
}
