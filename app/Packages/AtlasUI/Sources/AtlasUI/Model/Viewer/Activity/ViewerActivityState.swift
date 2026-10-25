/// The Activity pane: how many actions were done today, the search and the filters, and the lines in their groups, or
/// why there are none.
public struct ViewerActivityState: Equatable, Sendable {
  /// The note beside the title, such as "38 actions today"; empty while the log holds nothing.
  public var meta: String
  /// The search, as the person typed it.
  public var query: String
  /// The filters, and which one is chosen.
  public var filters: CapsuleTabsState<ViewerActivityFilter>
  /// The lines in their groups, newest first.
  public var groups: [ViewerActivityGroup]
  /// Why no line shows, in place of them; nil while lines show or before the first reading.
  public var empty: EmptyMessageState?
  /// Whether the log holds anything to search or filter, so the search and the filters show.
  public var isSearchable: Bool
  /// Counts Find's asks for the search field's focus while the pane shows.
  public var findRequests: Int

  /// Creates the Activity pane's state.
  ///
  /// - Parameters:
  ///   - meta: The note beside the title.
  ///   - query: The search.
  ///   - filters: The filters, and which one is chosen.
  ///   - groups: The lines in their groups.
  ///   - empty: Why no line shows.
  ///   - isSearchable: Whether the search and the filters show; they do by default.
  ///   - findRequests: Find's asks counted; none by default.
  public init(
    meta: String,
    query: String,
    filters: CapsuleTabsState<ViewerActivityFilter>,
    groups: [ViewerActivityGroup],
    empty: EmptyMessageState? = nil,
    isSearchable: Bool = true,
    findRequests: Int = 0
  ) {
    self.meta = meta
    self.query = query
    self.filters = filters
    self.groups = groups
    self.empty = empty
    self.isSearchable = isSearchable
    self.findRequests = findRequests
  }
}
