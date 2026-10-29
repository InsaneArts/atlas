/// Which rows the Activity tab lists: every one, those that did not work, the screenshots taken, or the text typed
/// and keys pressed.
public enum ActivityFilter: Sendable, CaseIterable {
  /// Every row.
  case all
  /// What did not work: a failed row, or one whose outcome is unknown. A row refused before anything was sent is not
  /// a problem.
  case problems
  /// The screenshots taken.
  case screenshots
  /// The text typed and the keys pressed.
  case typing

  /// Whether the filter lists `entry`.
  ///
  /// - Parameter entry: A row of the log.
  /// - Returns: Whether it passes: a failed one under Problems, a screenshot under Screenshots, and typing or keys
  ///   under Typing.
  func admits(_ entry: ActivityLog.Entry) -> Bool {
    switch self {
    case .all: true
    case .problems: entry.outcome == .failed
    case .screenshots: entry.operation == ActivityGrouping.screenshotOperation
    case .typing: [.typing, .keys].contains(ActivityAction(entry))
    }
  }
}
