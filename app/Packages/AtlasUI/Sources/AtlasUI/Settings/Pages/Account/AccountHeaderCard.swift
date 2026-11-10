import SwiftUI

/// The card at the top of an account's page: its first letter on its name's tinted tile, its name over what it is.
struct AccountHeaderCard: View {
  /// Its first letter on its tint.
  let monogram: AppMonogram
  /// Its name.
  let title: String
  /// What it is and when it was added.
  let line: String

  /// The disc and the words on the card.
  var body: some View {
    HStack(spacing: Spacing.m) {
      // The tile's body stands 34 across; its icon-grid margin overhangs, clear.
      MonogramTile(monogram: monogram, side: IconSize.xl / IconGrid.bodyShare)
        .frame(width: IconSize.xl, height: IconSize.xl)
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
