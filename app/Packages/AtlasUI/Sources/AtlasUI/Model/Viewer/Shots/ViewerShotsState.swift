/// The Screenshots pane: how many were taken today, the screenshots in their groups or why there are none, and the one
/// shown large.
public struct ViewerShotsState: Equatable, Sendable {
  /// The note beside the title, such as "6 today"; empty while none was taken.
  public var meta: String
  /// The screenshots in their groups, newest first.
  public var days: [ViewerShotDay]
  /// Why no screenshot shows, in place of them; nil while some show or before the first reading.
  public var empty: EmptyMessageState?
  /// The screenshot shown large over the whole window; nil shows none.
  public var large: ViewerLargeShot?

  /// Creates the Screenshots pane's state.
  ///
  /// - Parameters:
  ///   - meta: The note beside the title.
  ///   - days: The screenshots in their groups.
  ///   - empty: Why no screenshot shows.
  ///   - large: The screenshot shown large.
  public init(
    meta: String,
    days: [ViewerShotDay],
    empty: EmptyMessageState? = nil,
    large: ViewerLargeShot? = nil
  ) {
    self.meta = meta
    self.days = days
    self.empty = empty
    self.large = large
  }
}
