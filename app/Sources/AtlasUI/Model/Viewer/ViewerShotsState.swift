/// The Screenshots pane: how many were taken today, the search by time, the screenshots in their groups or why there
/// are none, and the one shown large.
package struct ViewerShotsState: Equatable, Sendable {
  /// The note beside the title, such as "6 today · Space opens one, drag one out to use it".
  package var meta: String
  /// The search by time, as the person typed it.
  package var query: String
  /// The screenshots in their groups, newest first.
  package var days: [ViewerShotDay]
  /// Why no screenshot shows, in place of them; nil while some show or before the first reading.
  package var empty: EmptyMessageState?
  /// The screenshot shown large over the whole window; nil shows none.
  package var large: ViewerLargeShot?

  /// Creates the Screenshots pane's state.
  ///
  /// - Parameters:
  ///   - meta: The note beside the title.
  ///   - query: The search by time.
  ///   - days: The screenshots in their groups.
  ///   - empty: Why no screenshot shows.
  ///   - large: The screenshot shown large.
  package init(
    meta: String,
    query: String,
    days: [ViewerShotDay],
    empty: EmptyMessageState? = nil,
    large: ViewerLargeShot? = nil
  ) {
    self.meta = meta
    self.query = query
    self.days = days
    self.empty = empty
    self.large = large
  }
}
