import SwiftUI

/// An account's level as a popup, where the three segments don't fit: the level's word and a chevron on a quiet
/// capsule, opening Look Only, Ask Before Sending and Trusted with the current one checked, then Remove from List….
struct AccountLevelMenu: View {
  /// The account's name, which VoiceOver reads with the popup.
  let account: String
  /// The level now.
  let level: AccountLevelChoice
  /// Whether its choices take presses now.
  let isEnabled: Bool
  /// Receives the level the person picks.
  let pick: @MainActor (AccountLevelChoice) -> Void
  /// Asks to take the account off the list.
  let remove: @MainActor () -> Void

  /// The popup.
  var body: some View {
    Menu {
      Picker("Level", selection: Binding(get: { level }, set: { if $0 != level { pick($0) } })) {
        ForEach(AccountLevelChoice.allCases, id: \.self) { choice in
          Text(choice.title)
            .tag(choice)
        }
      }
      .pickerStyle(.inline)
      .labelsHidden()
      Divider()
      Button("Remove from List…", role: .destructive, action: remove)
    } label: {
      HStack(spacing: Spacing.xsPlus) {
        Text(level.title)
          .font(Typography.font(.callout))
        Image(systemName: SymbolName.popup.rawValue)
          .font(Typography.font(.footnote, weight: .semibold))
          .accessibilityHidden(true)
      }
      .foregroundStyle(Color.textPrimary)
      .padding(.horizontal, Spacing.sPlus)
      .frame(height: ControlHeight.xs)
      .background(
        Capsule()
          .fill(Color.fillControl)
      )
      .contentShape(Capsule())
    }
    .menuStyle(.button)
    .buttonStyle(.plain)
    .menuIndicator(.hidden)
    .fixedSize()
    .disabled(!isEnabled)
    .accessibilityLabel("What agents may do in \(account)")
    .accessibilityValue(level.title)
  }
}
