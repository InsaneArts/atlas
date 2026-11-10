import SwiftUI

/// Why the Accounts page lists nothing, on a group's card: a quiet symbol, a headline and a sentence, centred, read as
/// one.
struct AccountsEmptyCard: View {
  /// What the card says.
  let state: EmptyMessageState

  /// The symbol, the headline and the sentence on the card.
  var body: some View {
    VStack(spacing: Spacing.xsPlus) {
      Image(systemName: SymbolName.personCircle.rawValue)
        .font(Typography.font(size: SettingsLayout.emptySymbol, weight: .light))
        .foregroundStyle(Color.textSecondary)
        .padding(.bottom, Spacing.s)
        .accessibilityHidden(true)
      Text(state.headline)
        .textStyle(.heading)
        .foregroundStyle(Color.textPrimary)
      if !state.message.isEmpty {
        Text(state.message)
          .textStyle(.body)
          .foregroundStyle(Color.textSecondary)
          .frame(maxWidth: SettingsLayout.emptyTextWidth)
          .fixedSize(horizontal: false, vertical: true)
      }
    }
    .multilineTextAlignment(.center)
    .frame(maxWidth: .infinity)
    .padding(.vertical, Spacing.xxl)
    .padding(.horizontal, Spacing.xxxl)
    .background(
      RoundedRectangle(cornerRadius: Radius.large, style: .continuous)
        .fill(Color.fillGroupInset)
    )
    .accessibilityElement(children: .combine)
  }
}
