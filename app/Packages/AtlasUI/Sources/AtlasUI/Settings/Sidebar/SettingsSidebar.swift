import SwiftUI

/// The Settings sidebar, straight on the window's recessed ground as a source list stands: the window buttons' band,
/// the search, then a row for each page the search leaves, 13 apart between groups, the page shown on its raised
/// capsule. Escape empties the search; a search that finds nothing says so.
struct SettingsSidebar: View {
  /// The rows and the search.
  let state: SettingsSidebarState
  /// The page shown, an account's page showing under Accounts.
  let page: SettingsPage
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// The band, the search and the rows.
  var body: some View {
    VStack(alignment: .leading, spacing: .zero) {
      SearchField(text: state.query, prompt: "Search Settings", fills: true) { send(.search($0)) }
        .padding(.bottom, SettingsLayout.searchGap)
      rows
      Spacer(minLength: .zero)
    }
    .padding(.top, WindowChrome.titleBand)
    .padding(.horizontal, SettingsLayout.sidebarInset)
    .frame(width: SettingsLayout.sidebarWidth)
    .accessibilityElement(children: .contain)
    .accessibilityLabel(Text(verbatim: "Settings"))
  }

  /// The rows the search leaves, a gap before each new group, or the line saying none matched.
  @ViewBuilder private var rows: some View {
    let visible = state.visibleRows
    if visible.isEmpty {
      Text(verbatim: "No Results")
        .textStyle(.callout)
        .foregroundStyle(Color.textSecondary)
        .frame(maxWidth: .infinity)
        .padding(.top, Spacing.m)
    } else {
      VStack(alignment: .leading, spacing: .zero) {
        ForEach(Array(visible.enumerated()), id: \.element.id) { index, row in
          if index > 0, visible[index - 1].page.group != row.page.group {
            Color.clear
              .frame(height: SettingsLayout.groupGap)
          }
          SettingsSidebarRowView(row: row, isSelected: row.page == page) { send(.select(row.page)) }
        }
      }
    }
  }
}
