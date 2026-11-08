/// The Activity pane: whose work it lists and how much was done today for VoiceOver, the filters, and the outline of
/// sessions, app runs and steps by day, or why there is none; the picture sheet and the replay over it. The sidebar's
/// search narrows the steps.
public struct ViewerActivityState: Equatable, Sendable {
  /// What VoiceOver calls the pane: "Activity", or the chosen session's name.
  public var title: String
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

  /// Creates the Activity pane's state.
  ///
  /// - Parameters:
  ///   - title: What VoiceOver calls the pane; "Activity" by default.
  ///   - meta: What VoiceOver says of it.
  ///   - filters: The filters, and which one is chosen.
  ///   - days: The days and their sessions, runs and steps.
  ///   - empty: Why nothing shows.
  ///   - query: The sidebar's search narrowing the steps; none by default.
  ///   - showsFilters: Whether the filters show; they do by default.
  ///   - sheet: The picture sheet; none by default.
  ///   - replay: The task replayed; none by default.
  public init(
    title: String = "Activity",
    meta: String,
    filters: CapsuleTabsState<ViewerActivityFilter>,
    days: [ViewerActivityDay],
    empty: EmptyMessageState? = nil,
    query: String = "",
    showsFilters: Bool = true,
    sheet: ViewerPictureSheet? = nil,
    replay: ViewerReplayState? = nil
  ) {
    self.title = title
    self.meta = meta
    self.filters = filters
    self.days = days
    self.empty = empty
    self.query = query
    self.showsFilters = showsFilters
    self.sheet = sheet
    self.replay = replay
  }

  /// The newest run's identity, whose change is new work arriving at the top.
  public var newest: String? {
    days.lazy
      .flatMap(\.sessions)
      .flatMap(\.runs)
      .first?.id
  }
}
