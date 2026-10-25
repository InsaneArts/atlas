/// The Apps pane: how many apps and windows are open, the search, and the apps front to back, or why there are none.
public struct ViewerAppsState: Equatable, Sendable {
  /// The note beside the title, such as "5 apps · 6 windows"; empty while no app is open.
  public var meta: String
  /// The search, as the person typed it.
  public var query: String
  /// The apps, front to back.
  public var apps: [ViewerAppRow]
  /// Why no app shows, in place of them; nil while apps show or before the first reading.
  public var empty: EmptyMessageState?
  /// Whether any app is open to search, so the search shows.
  public var isSearchable: Bool
  /// Counts Find's asks for the search field's focus while the pane shows.
  public var findRequests: Int

  /// Creates the Apps pane's state.
  ///
  /// - Parameters:
  ///   - meta: The note beside the title.
  ///   - query: The search.
  ///   - apps: The apps, front to back.
  ///   - empty: Why no app shows.
  ///   - isSearchable: Whether the search shows; it does by default.
  ///   - findRequests: Find's asks counted; none by default.
  public init(
    meta: String,
    query: String,
    apps: [ViewerAppRow],
    empty: EmptyMessageState? = nil,
    isSearchable: Bool = true,
    findRequests: Int = 0
  ) {
    self.meta = meta
    self.query = query
    self.apps = apps
    self.empty = empty
    self.isSearchable = isSearchable
    self.findRequests = findRequests
  }
}
