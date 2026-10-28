/// The Activity pane: its title, how many actions were done today, the search and the filters, and the lines in their
/// groups, by time or by host session, or why there are none.
public struct ViewerActivityState: Equatable, Sendable {
  /// The most lines a host session's group lists before Show all.
  public static let mostSessionLines = ViewerLayout.groupLines

  /// The pane's title: "Activity", or the chosen session's name.
  public var title: String
  /// The chosen session's icon before the title; nil for none.
  public var titleIcon: RowIcon?
  /// The note beside the title, such as "38 actions today"; empty while the log holds nothing.
  public var meta: String
  /// The search, as the person typed it.
  public var query: String
  /// The filters, and which one is chosen.
  public var filters: CapsuleTabsState<ViewerActivityFilter>
  /// The lines in their groups, newest first.
  public var groups: [ViewerActivityGroup]
  /// The lines under each host session, drawn in place of ``groups`` while there are any.
  public var sessions: [ViewerActivitySessionGroup]
  /// Why no line shows, in place of them; nil while lines show or before the first reading.
  public var empty: EmptyMessageState?
  /// Whether the log holds anything to search or filter, so the search and the filters show.
  public var isSearchable: Bool
  /// Counts Find's asks for the search field's focus while the pane shows.
  public var findRequests: Int

  /// Creates the Activity pane's state.
  ///
  /// - Parameters:
  ///   - title: The pane's title; "Activity" by default.
  ///   - titleIcon: The chosen session's icon; none by default.
  ///   - meta: The note beside the title.
  ///   - query: The search.
  ///   - filters: The filters, and which one is chosen.
  ///   - groups: The lines in their groups.
  ///   - sessions: The lines under each host session; none by default.
  ///   - empty: Why no line shows.
  ///   - isSearchable: Whether the search and the filters show; they do by default.
  ///   - findRequests: Find's asks counted; none by default.
  public init(
    title: String = "Activity",
    titleIcon: RowIcon? = nil,
    meta: String,
    query: String,
    filters: CapsuleTabsState<ViewerActivityFilter>,
    groups: [ViewerActivityGroup],
    sessions: [ViewerActivitySessionGroup] = [],
    empty: EmptyMessageState? = nil,
    isSearchable: Bool = true,
    findRequests: Int = 0
  ) {
    self.title = title
    self.titleIcon = titleIcon
    self.meta = meta
    self.query = query
    self.filters = filters
    self.groups = groups
    self.sessions = sessions
    self.empty = empty
    self.isSearchable = isSearchable
    self.findRequests = findRequests
  }
}
