import SwiftUI

/// The card at the top of a VM's page, as the Helper page's stands: Atlas's mark with the VM's dot, its title over
/// what it is. It has no button.
struct VMStatusCard: View {
  /// The page.
  let state: SettingsVMState

  /// What it draws, from its state.
  var body: some View {
    HStack(spacing: Spacing.m) {
      HelperMark(
        tone: state.tone,
        tile: HelperMarkLayout.cardTile,
        ground: .fillContent,
        wash: .fillGroupInset
      )
      VStack(alignment: .leading, spacing: Spacing.hairline) {
        Text(state.title)
          .textStyle(.body, weight: .semibold)
          .foregroundStyle(Color.textPrimary)
          .lineLimit(1)
        Text(state.line)
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
