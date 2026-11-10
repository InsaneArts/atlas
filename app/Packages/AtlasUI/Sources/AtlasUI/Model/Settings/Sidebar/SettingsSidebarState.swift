/// The Settings window's sidebar: a row for each page.
public struct SettingsSidebarState: Equatable, Sendable {
  /// Every page's row, in order.
  public var rows: [SettingsSidebarRow]

  /// Creates the sidebar's state.
  ///
  /// - Parameter rows: Every page's row, in order.
  public init(rows: [SettingsSidebarRow]) {
    self.rows = rows
  }

  /// A page's title, as its row has it; empty for a page with no row.
  ///
  /// - Parameter page: The page.
  /// - Returns: Its title.
  public func title(of page: SettingsPage) -> String {
    rows.first { $0.page == page }?.title ?? ""
  }
}
