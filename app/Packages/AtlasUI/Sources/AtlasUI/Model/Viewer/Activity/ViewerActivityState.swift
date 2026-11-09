/// The Activity pane: whose work it lists and how much was done today for VoiceOver, the filters, and the outline of
/// sessions, app runs and steps by day, or why there is none; the picture sheet and the replay over it. The sidebar's
/// search narrows the steps.
public struct ViewerActivityState: Equatable, Sendable {
  /// What VoiceOver calls the pane: "Activity", or the chosen session's name.
  public var title: String
  /// Whose work the list shows: every session's, or the chosen one's.
  public var session: ViewerSessionChoice
  /// What VoiceOver says of it, such as "38 actions today"; empty while the log holds nothing.
  public var meta: String
  /// The filters, and which one is chosen.
  public var filters: CapsuleTabsState<ViewerActivityFilter>
  /// The days and their sessions, newest first; each session owns its runs, and each run its steps.
  public var days: [ViewerActivityDay]
  /// Why nothing shows, in place of the items; nil while items show or before the first reading.
  public var empty: EmptyMessageState?
  /// The sidebar's search narrowing the steps; empty for none.
  public var query: String
  /// Whether the log holds anything to filter, so the filters show.
  public var showsFilters: Bool
  /// The picture sheet over the window; nil shows none.
  public var sheet: ViewerPictureSheet?
  /// The task replayed over the whole window; nil shows none.
  public var replay: ViewerReplayState?
  /// Whether the rows are a reading of whose work the pane lists: false while a reading for an earlier choice shows
  /// what it holds of this one, until this one's own arrives, so its rows replace them as one list, never as new work.
  public var settled: Bool

  /// Creates the Activity pane's state.
  ///
  /// - Parameters:
  ///   - title: What VoiceOver calls the pane; "Activity" by default.
  ///   - session: Whose work the list shows; every session's by default.
  ///   - meta: What VoiceOver says of it.
  ///   - filters: The filters, and which one is chosen.
  ///   - days: The days and their sessions, runs and steps.
  ///   - empty: Why nothing shows.
  ///   - query: The sidebar's search narrowing the steps; none by default.
  ///   - showsFilters: Whether the filters show; they do by default.
  ///   - sheet: The picture sheet; none by default.
  ///   - replay: The task replayed; none by default.
  ///   - settled: Whether the rows are a reading of whose work the pane lists; they are by default.
  public init(
    title: String = "Activity",
    session: ViewerSessionChoice = .all,
    meta: String,
    filters: CapsuleTabsState<ViewerActivityFilter>,
    days: [ViewerActivityDay],
    empty: EmptyMessageState? = nil,
    query: String = "",
    showsFilters: Bool = true,
    sheet: ViewerPictureSheet? = nil,
    replay: ViewerReplayState? = nil,
    settled: Bool = true
  ) {
    self.title = title
    self.session = session
    self.meta = meta
    self.filters = filters
    self.days = days
    self.empty = empty
    self.query = query
    self.showsFilters = showsFilters
    self.sheet = sheet
    self.replay = replay
    self.settled = settled
  }

  /// Which list shows: whose work, under which filter, and whether it is that work's own reading yet. A change of it
  /// is the person's own, or their choice's reading arriving, which swaps the whole list rather than bringing rows in.
  public var list: ViewerActivityListKey {
    ViewerActivityListKey(session: session, filter: filters.selected, settled: settled)
  }

  /// The newest run's identity, whose change is new work arriving at the top.
  public var newest: String? {
    days.lazy
      .flatMap(\.sessions)
      .flatMap(\.runs)
      .first?.id
  }
}
