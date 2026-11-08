import SwiftUI

/// The note Atlas shows once after it added an account: a key, what it added and that agents ask before sending
/// there, Change…, which opens the account's levels, and OK, on a card tinted with the accent.
struct AccountNoteCard: View {
  /// What it says.
  let text: String
  /// The account's level now, which Change… opens on; nil offers no Change….
  let level: AccountLevelChoice?
  /// Receives the level picked from Change….
  let pick: @MainActor (AccountLevelChoice) -> Void
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
      if let level {
        Menu {
          Picker("Level", selection: Binding(get: { level }, set: { pick($0) })) {
            ForEach(AccountLevelChoice.allCases, id: \.self) { choice in
              Text(choice.title)
                .tag(choice)
            }
          }
          .pickerStyle(.inline)
          .labelsHidden()
        } label: {
          Text("Change…")
        }
        .menuStyle(.button)
        .menuIndicator(.hidden)
        .buttonStyle(.capsule(.secondary, size: .mini))
        .fixedSize()
      }
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
