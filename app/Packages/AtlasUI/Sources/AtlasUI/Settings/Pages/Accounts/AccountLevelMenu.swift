import SwiftUI

/// An account's level as a popup, where the three segments don't fit: the level's word and a chevron on a quiet
/// capsule, opening Look Only, Ask Before Sending and Trusted with the current one checked, then what else can be done
/// with the account.
struct AccountLevelMenu<Actions: View>: View {
  /// The account's name, which VoiceOver reads with the popup.
  let account: String
  /// The level now.
  let level: AccountLevelChoice
  /// Whether its choices take presses now.
  let isEnabled: Bool
  /// Receives the level the person picks.
  let pick: @MainActor (AccountLevelChoice) -> Void
  /// The items after the levels, such as Remove from List….
  @ViewBuilder let actions: () -> Actions

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
      actions()
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
