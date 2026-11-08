import SwiftUI

/// The note Atlas shows once after it added an account: a key, what it added and why, and OK, on a card tinted with
/// the accent.
struct AccountNoteCard: View {
  /// What it says.
  let text: String
  /// The person read it.
  let dismiss: @MainActor () -> Void

  /// What it draws.
  var body: some View {
    HStack(spacing: ListRow.gap) {
      Image(systemName: SymbolName.key.rawValue)
        .font(Typography.font(size: IconSize.s))
        .foregroundStyle(Color.accent)
        .frame(width: IconSize.s, height: IconSize.s)
        .accessibilityHidden(true)
      Text(text)
        .textStyle(.body)
        .foregroundStyle(Color.textPrimary)
        .frame(maxWidth: .infinity, alignment: .leading)
      Button("OK", action: dismiss)
        .buttonStyle(.capsule(.secondary, size: .mini))
    }
    .padding(Spacing.m)
    .background(
      RoundedRectangle(cornerRadius: Radius.large, style: .continuous)
        .fill(Color.accentWash)
    )
    .accessibilityElement(children: .combine)
  }
}
