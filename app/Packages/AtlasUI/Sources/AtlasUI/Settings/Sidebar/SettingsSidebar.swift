import SwiftUI

/// The Settings sidebar, straight on the window's recessed ground as a source list stands: the window buttons' band,
/// then a row for each page, 13 apart between groups, the page shown on its raised capsule.
struct SettingsSidebar: View {
  /// The rows.
  let state: SettingsSidebarState
  /// The page shown, an account's page showing under Accounts.
  let page: SettingsPage
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// The band and the rows.
  var body: some View {
    VStack(alignment: .leading, spacing: .zero) {
      ForEach(Array(state.rows.enumerated()), id: \.element.id) { index, row in
        if index > 0, state.rows[index - 1].page.group != row.page.group {
          Color.clear
            .frame(height: SettingsLayout.groupGap)
        }
        SettingsSidebarRowView(row: row, isSelected: row.page == page) { send(.select(row.page)) }
      }
      Spacer(minLength: .zero)
    }
    .padding(.top, WindowChrome.titleBand)
    .padding(.horizontal, SettingsLayout.sidebarInset)
    .frame(width: SettingsLayout.sidebarWidth)
    .accessibilityElement(children: .contain)
    .accessibilityLabel(Text(verbatim: "Settings"))
  }
}
