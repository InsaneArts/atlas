/// The sidebar's sessions: whose work shows, All sessions with the first live sessions' icons and how many are live,
/// then each section that holds a session, Needs you first, and Ended last with its count, folded until the person
/// opens it.
public struct ViewerSessionsState: Equatable, Sendable {
  /// Whose work shows now, which the sidebar draws selected.
  public var chosen: ViewerSessionChoice
  /// The icons All sessions stacks, at most three: the first live sessions'.
  public var icons: [RowIcon]
  /// How many sessions are live, after All sessions; none says nothing.
  public var liveCount: Int
  /// The sections that hold a session, in the order they stand; Ended's while any session ended.
  public var groups: [ViewerSessionGroup]
  /// How many sessions ended, beside Ended's heading.
  public var endedCount: Int
  /// Whether Ended is open, listing every session that ended rather than only the chosen one.
  public var endedShown: Bool

  /// Creates the sessions' state.
  ///
  /// - Parameters:
  ///   - chosen: Whose work shows now; every session's by default.
  ///   - icons: The icons All sessions stacks.
  ///   - liveCount: How many sessions are live.
  ///   - groups: The sections that hold a session, in order.
  ///   - endedCount: How many sessions ended; none by default.
  ///   - endedShown: Whether Ended is open; folded by default.
  public init(
    chosen: ViewerSessionChoice = .all,
    icons: [RowIcon],
    liveCount: Int,
    groups: [ViewerSessionGroup],
    endedCount: Int = 0,
    endedShown: Bool = false
  ) {
    self.chosen = chosen
    self.icons = icons
    self.liveCount = liveCount
    self.groups = groups
    self.endedCount = endedCount
    self.endedShown = endedShown
  }

  /// The rows `section` draws, none when it holds no session.
  ///
  /// - Parameter section: The section.
  /// - Returns: Its rows, in order.
  public func rows(_ section: ViewerSessionSection) -> [ViewerSessionRow] {
    groups.first { $0.section == section }?
      .rows ?? []
  }

  /// Every choice the sidebar draws, top to bottom: every session's work, then each row's in the order the sections
  /// draw them.
  public var choices: [ViewerSessionChoice] {
    [.all] + groups.flatMap { group in group.rows.map { .one($0.id) } }
  }

  /// The choice `offset` rows from `origin`, as ↑ and ↓ step: held at the first and last rows drawn, and from the
  /// top when `origin` has no row.
  ///
  /// - Parameters:
  ///   - offset: How many rows down; negative goes up.
  ///   - origin: The row stepped from: the one with the keyboard focus, or the one chosen.
  /// - Returns: The choice there.
  public func choice(_ offset: Int, from origin: ViewerSessionChoice) -> ViewerSessionChoice {
    let choices = choices
    let index = choices.firstIndex(of: origin) ?? .zero
    return choices[min(max(index + offset, .zero), choices.count - 1)]
  }
}
