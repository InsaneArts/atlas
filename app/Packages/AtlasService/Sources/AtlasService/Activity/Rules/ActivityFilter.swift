/// Which rows the Activity pane lists: every one, those that did not work, the pictures it took, or the text typed and
/// keys pressed.
public enum ActivityFilter: Sendable, CaseIterable {
  /// Every row.
  case all
  /// What did not work: a failed row, or one whose outcome is unknown. A row refused before anything was sent is not
  /// a problem.
  case problems
  /// The screenshots taken: the steps that show a picture in the outline.
  case pictures
  /// The text typed and the keys pressed.
  case typing

  /// Whether the filter lists `entry`.
  ///
  /// - Parameter entry: A row of the log.
  /// - Returns: Whether it passes: a failed one under Problems, a screenshot under Pictures, and typing or keys
  ///   under Typing.
  func admits(_ entry: ActivityLog.Entry) -> Bool {
    switch self {
    case .all: true
    case .problems: entry.outcome == .failed
    case .pictures: entry.operation == ActivityGrouping.screenshotOperation
    case .typing: [.typing, .keys].contains(ActivityAction(entry))
    }
  }
}
