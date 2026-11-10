/// One of the Settings sidebar's rows: a page, its title, and for Helper the line under it saying how the helper
/// stands, with its dot's tone.
public struct SettingsSidebarRow: Equatable, Sendable, Identifiable {
  /// The page the row shows.
  public var page: SettingsPage
  /// The page's title, such as "General".
  public var title: String
  /// The line under the title, such as "Ready"; nil for none.
  public var subtitle: String?
  /// The tone of the helper's dot, for the Helper row; quiet elsewhere.
  public var tone: StatusTone

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - page: The page the row shows.
  ///   - title: The page's title.
  ///   - subtitle: The line under the title; none by default.
  ///   - tone: The tone of the helper's dot; quiet by default.
  public init(page: SettingsPage, title: String, subtitle: String? = nil, tone: StatusTone = .quiet) {
    self.page = page
    self.title = title
    self.subtitle = subtitle
    self.tone = tone
  }

  /// One row for each page.
  public var id: SettingsPage { page }
}
