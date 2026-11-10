/// The Settings window's sidebar: a row for each page and the search over them.
public struct SettingsSidebarState: Equatable, Sendable {
  /// Every page's row, in order.
  public var rows: [SettingsSidebarRow]
  /// What the person typed in the search; empty for none.
  public var query: String

  /// Creates the sidebar's state.
  ///
  /// - Parameters:
  ///   - rows: Every page's row, in order.
  ///   - query: What the person typed in the search; none by default.
  public init(rows: [SettingsSidebarRow], query: String = "") {
    self.rows = rows
    self.query = query
  }

  /// The rows the search leaves: those whose title or words hold the query, every row while it is empty.
  public var visibleRows: [SettingsSidebarRow] {
    rows.filter { $0.page.matches(query, title: $0.title) }
  }

  /// A page's title, as its row has it; empty for a page with no row.
  ///
  /// - Parameter page: The page.
  /// - Returns: Its title.
  public func title(of page: SettingsPage) -> String {
    rows.first { $0.page == page }?.title ?? ""
  }
}
