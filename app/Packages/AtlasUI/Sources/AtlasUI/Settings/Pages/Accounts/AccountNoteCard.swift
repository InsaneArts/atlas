import SwiftUI

/// The note Atlas shows once after it added an account, on a group's card: a plus on an accent tile, what it added and
/// what agents may do there, Change…, which opens the account's page, and OK.
struct AccountNoteCard: View {
  /// What it says.
  let text: String
  /// Whether Change… shows: the note names an account listed.
  let canChange: Bool
  /// Opens the account's page.
  let change: @MainActor () -> Void
  /// The person read it.
  let dismiss: @MainActor () -> Void

  /// What it draws.
  var body: some View {
    HStack(spacing: ListRow.Size.compact.gap) {
      Image(systemName: SymbolName.plus.rawValue)
        .font(Typography.font(.callout, weight: .bold))
        .foregroundStyle(Color.onAccent)
        .frame(width: IconSize.m, height: IconSize.m)
        .background(
          RoundedRectangle(cornerRadius: Radius.extraSmall, style: .continuous)
            .fill(Color.accent)
        )
        .accessibilityHidden(true)
      Text(text)
        .textStyle(.body)
        .foregroundStyle(Color.textPrimary)
        .fixedSize(horizontal: false, vertical: true)
        .frame(maxWidth: .infinity, alignment: .leading)
      if canChange {
        Button("Change…", action: change)
          .buttonStyle(.capsule(.secondary, size: .row))
      }
      Button("OK", action: dismiss)
        .buttonStyle(.capsule(.accent, size: .row))
    }
    .padding(.horizontal, ListRow.Size.compact.inset)
    .padding(.vertical, Spacing.m)
    .background(
      RoundedRectangle(cornerRadius: Radius.large, style: .continuous)
        .fill(Color.fillGroupInset)
    )
    .accessibilityElement(children: .contain)
  }
}
