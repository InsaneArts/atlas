/// The Activity pane: how many actions were done today, the search and the filters, and the lines in their groups, or
/// why there are none.
public struct ViewerActivityState: Equatable, Sendable {
  /// The note beside the title, such as "38 actions today".
  public var meta: String
  /// The search, as the person typed it.
  public var query: String
  /// The filters, and which one is chosen.
  public var filters: CapsuleTabsState<ViewerActivityFilter>
  /// The lines in their groups, newest first.
  public var groups: [ViewerActivityGroup]
  /// Why no line shows, in place of them; nil while lines show or before the first reading.
  public var empty: EmptyMessageState?

  /// Creates the Activity pane's state.
  ///
  /// - Parameters:
  ///   - meta: The note beside the title.
  ///   - query: The search.
  ///   - filters: The filters, and which one is chosen.
  ///   - groups: The lines in their groups.
  ///   - empty: Why no line shows.
  public init(
    meta: String,
    query: String,
    filters: CapsuleTabsState<ViewerActivityFilter>,
    groups: [ViewerActivityGroup],
    empty: EmptyMessageState? = nil
  ) {
    self.meta = meta
    self.query = query
    self.filters = filters
    self.groups = groups
    self.empty = empty
  }
}
