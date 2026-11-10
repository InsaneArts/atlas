import SwiftUI

/// The card at the top of an account's page: its app's or site's icon, else its first letter on its name's tinted
/// tile, its name over what it is.
struct AccountHeaderCard: View {
  /// Its app's or site's icon; nil for its first letter.
  let icon: SettingsAccountIcon?
  /// Its first letter.
  let letter: String
  /// Its name.
  let title: String
  /// What it is and when it was added.
  let line: String

  /// The disc and the words on the card.
  var body: some View {
    HStack(spacing: Spacing.m) {
      AccountTile(icon: icon, letter: letter, title: title)
      VStack(alignment: .leading, spacing: Spacing.hairline) {
        // A long name wraps whole, the card growing to hold it.
        Text(title)
          .textStyle(.body, weight: .semibold)
          .foregroundStyle(Color.textPrimary)
          .fixedSize(horizontal: false, vertical: true)
        Text(line)
          .textStyle(.caption)
          .foregroundStyle(Color.textSecondary)
          .lineLimit(1)
      }
      .frame(maxWidth: .infinity, alignment: .leading)
    }
    .padding(.horizontal, ListRow.Size.compact.inset)
    .padding(.vertical, Spacing.m)
    .frame(minHeight: SettingsLayout.headerRowHeight)
    .background(
      RoundedRectangle(cornerRadius: Radius.large, style: .continuous)
        .fill(Color.fillGroupInset)
    )
    .accessibilityElement(children: .combine)
  }
}
