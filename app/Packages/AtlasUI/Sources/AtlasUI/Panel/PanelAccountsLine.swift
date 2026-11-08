import SwiftUI

/// The panel's one quiet line of the accounts Atlas lists for the helper, such as "Accounts: github.com and Slack":
/// a key, the words and a chevron, the whole row a button that opens Settings › Accounts, its hover concentric with
/// the panel's corner. VoiceOver reads it as one button.
struct PanelAccountsLine: View {
  /// The words.
  let line: String
  /// Receives the press.
  let send: @MainActor (PanelEvent) -> Void

  /// The key, the words and the chevron.
  var body: some View {
    Button {
      send(.openAccounts)
    } label: {
      HStack(spacing: Spacing.s) {
        Image(systemName: SymbolName.key.rawValue)
          .font(Typography.font(size: IconSize.xs))
          .foregroundStyle(Color.textSecondary)
          .frame(width: PanelLayout.recentIcon)
        Text(line)
          .truncationMode(.tail)
          .foregroundStyle(Color.textSecondary)
          .frame(maxWidth: .infinity, alignment: .leading)
        Image(systemName: SymbolName.next.rawValue)
          .font(Typography.font(size: IconSize.xs))
          .foregroundStyle(Color.textTertiary)
      }
      .font(Typography.font(.callout))
      .lineLimit(1)
      .frame(minHeight: PanelLayout.recentRow)
      .padding(.horizontal, PanelLayout.textInset)
      .contentShape(Rectangle())
    }
    .buttonStyle(.row(corner: PanelLayout.sessionCorner))
    .accessibilityLabel(line)
    .accessibilityHint("Opens Settings › Accounts")
  }
}
