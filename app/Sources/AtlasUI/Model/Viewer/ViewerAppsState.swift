/// The Apps pane: how many apps and windows are open, the search, and the apps front to back, or why there are none.
package struct ViewerAppsState: Equatable, Sendable {
  /// The note beside the title, such as "5 apps · 6 windows, front to back".
  package var meta: String
  /// The search, as the person typed it.
  package var query: String
  /// The apps, front to back.
  package var apps: [ViewerAppRow]
  /// Why no app shows, in place of them; nil while apps show or before the first reading.
  package var empty: EmptyMessageState?

  /// Creates the Apps pane's state.
  ///
  /// - Parameters:
  ///   - meta: The note beside the title.
  ///   - query: The search.
  ///   - apps: The apps, front to back.
  ///   - empty: Why no app shows.
  package init(meta: String, query: String, apps: [ViewerAppRow], empty: EmptyMessageState? = nil) {
    self.meta = meta
    self.query = query
    self.apps = apps
    self.empty = empty
  }
}
