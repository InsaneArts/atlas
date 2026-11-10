import SwiftUI

/// The card at the top of an account's page: its first letter on a disc, its name over what it is.
struct AccountHeaderCard: View {
  /// The first letter of its name.
  let letter: String
  /// Its name.
  let title: String
  /// What it is and when it was added.
  let line: String

  /// The disc and the words on the card.
  var body: some View {
    HStack(spacing: Spacing.m) {
      Text(letter)
        .font(Typography.font(.subheading, weight: .semibold))
        .foregroundStyle(Color.textPrimary)
        .frame(width: IconSize.xl, height: IconSize.xl)
        .background(
          Circle()
            .fill(Color.fillAvatar)
        )
        .accessibilityHidden(true)
      VStack(alignment: .leading, spacing: Spacing.hairline) {
        Text(title)
          .textStyle(.body, weight: .semibold)
          .foregroundStyle(Color.textPrimary)
          .lineLimit(1)
          .truncationMode(.middle)
        Text(line)
          .textStyle(.caption)
          .foregroundStyle(Color.textSecondary)
          .lineLimit(1)
      }
      .frame(maxWidth: .infinity, alignment: .leading)
    }
    .padding(.horizontal, ListRow.Size.compact.inset)
    .frame(minHeight: SettingsLayout.headerRowHeight)
    .background(
      RoundedRectangle(cornerRadius: Radius.large, style: .continuous)
        .fill(Color.fillGroupInset)
    )
    .accessibilityElement(children: .combine)
  }
}
