import SwiftUI

/// One page in the Settings sidebar as a button on its capsule (``PillRowStyle``): the helper's mark with its dot and
/// how it stands under its title for Helper, else the page's tile and its title, semibold while the page shows.
/// VoiceOver reads its title as the button's and says whether it is selected.
struct SettingsSidebarRowView: View {
  /// The row.
  let row: SettingsSidebarRow
  /// Whether its page shows.
  let isSelected: Bool
  /// Shows its page.
  let select: @MainActor () -> Void

  /// The row as a button on its highlight.
  var body: some View {
    Button(action: select) {
      HStack(spacing: SettingsLayout.pageRowGap) {
        lead
        words
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(.horizontal, SettingsLayout.pageRowInset)
      .frame(height: row.page == .helper ? SettingsLayout.helperRowHeight : SettingsLayout.pageRowHeight)
    }
    .buttonStyle(.pill(isSelected: isSelected))
    .accessibilityLabel(Text(verbatim: row.subtitle.map { "\(row.title), \($0)" } ?? row.title))
    .accessibilityAddTraits(isSelected ? .isSelected : [])
  }

  /// The helper's mark, its dot's ring in the ground it stands on, or the page's tile.
  @ViewBuilder private var lead: some View {
    if row.page == .helper {
      SidebarHelperMark(tone: row.tone, ground: isSelected ? .fillSelection : .groundRecessed)
    } else {
      SettingsPageTile(page: row.page)
    }
  }

  /// The title, over its line when it has one.
  private var words: some View {
    VStack(alignment: .leading, spacing: .zero) {
      Text(row.title)
        .textStyle(.body, weight: isSelected ? .semibold : .regular)
        .foregroundStyle(Color.textPrimary)
      if let subtitle = row.subtitle {
        Text(subtitle)
          .textStyle(.caption)
          .foregroundStyle(Color.textSecondary)
      }
    }
    .lineLimit(1)
  }
}
