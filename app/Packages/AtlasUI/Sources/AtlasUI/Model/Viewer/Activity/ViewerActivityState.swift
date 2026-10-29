/// The Activity pane: whose work it lists and how much was done today for VoiceOver, the filters, and the moments with
/// their steps as one flat list, or why there are none. The sidebar's search narrows the steps.
public struct ViewerActivityState: Equatable, Sendable {
  /// What VoiceOver calls the pane: "Activity", or the chosen session's name.
  public var title: String
  /// What VoiceOver says of it, such as "38 actions today"; empty while the log holds nothing.
  public var meta: String
  /// The filters, and which one is chosen.
  public var filters: CapsuleTabsState<ViewerActivityFilter>
  /// The days, moments, steps and "Show more" rows, in the order they draw, newest first.
  public var items: [ViewerActivityItem]
  /// Why nothing shows, in place of the items; nil while items show or before the first reading.
  public var empty: EmptyMessageState?
  /// Whether the log holds anything to filter, so the filters show.
  public var showsFilters: Bool

  /// Creates the Activity pane's state.
  ///
  /// - Parameters:
  ///   - title: What VoiceOver calls the pane; "Activity" by default.
  ///   - meta: What VoiceOver says of it.
  ///   - filters: The filters, and which one is chosen.
  ///   - items: The days, moments, steps and "Show more" rows.
  ///   - empty: Why nothing shows.
  ///   - showsFilters: Whether the filters show; they do by default.
  public init(
    title: String = "Activity",
    meta: String,
    filters: CapsuleTabsState<ViewerActivityFilter>,
    items: [ViewerActivityItem],
    empty: EmptyMessageState? = nil,
    showsFilters: Bool = true
  ) {
    self.title = title
    self.meta = meta
    self.filters = filters
    self.items = items
    self.empty = empty
    self.showsFilters = showsFilters
  }

  /// The newest step's identity, whose change is a new step arriving at the top.
  public var newest: String? {
    items.lazy
      .compactMap { item -> String? in
        if case .step(let step) = item { return step.id }
        return nil
      }
      .first
  }
}
