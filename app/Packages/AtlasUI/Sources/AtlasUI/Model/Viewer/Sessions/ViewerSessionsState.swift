/// The main window's sessions column, beside every tab while two or more host sessions are live or one is chosen:
/// whose work shows, All sessions with the first live sessions' icons, every live session, and those that ended.
public struct ViewerSessionsState: Equatable, Sendable {
  /// Whose work shows now, which the column draws selected.
  public var chosen: ViewerSessionChoice
  /// The icons All sessions stacks, at most three: the first live sessions'.
  public var icons: [RowIcon]
  /// Every live session, in the service's order.
  public var live: [ViewerSessionRow]
  /// The sessions that ended, newest first; none hides their part.
  public var ended: [ViewerSessionRow]

  /// Creates the column's state.
  ///
  /// - Parameters:
  ///   - chosen: Whose work shows now; every session's by default.
  ///   - icons: The icons All sessions stacks.
  ///   - live: Every live session, in the service's order.
  ///   - ended: The sessions that ended, newest first; none by default.
  public init(
    chosen: ViewerSessionChoice = .all,
    icons: [RowIcon],
    live: [ViewerSessionRow],
    ended: [ViewerSessionRow] = []
  ) {
    self.chosen = chosen
    self.icons = icons
    self.live = live
    self.ended = ended
  }

  /// Every choice the column offers, top to bottom: every session's work, then each live session's, then each ended
  /// one's.
  var choices: [ViewerSessionChoice] {
    let rows = live + ended
    return [.all] + rows.map { .one($0.id) }
  }

  /// The choice `offset` rows from `origin`, as ↑ and ↓ step: held at the column's first and last rows, and from the
  /// top when `origin` has no row.
  ///
  /// - Parameters:
  ///   - offset: How many rows down; negative goes up.
  ///   - origin: The row stepped from: the one with the keyboard focus, or the one chosen.
  /// - Returns: The choice there.
  func choice(_ offset: Int, from origin: ViewerSessionChoice) -> ViewerSessionChoice {
    let choices = choices
    let index = choices.firstIndex(of: origin) ?? .zero
    return choices[min(max(index + offset, .zero), choices.count - 1)]
  }
}
