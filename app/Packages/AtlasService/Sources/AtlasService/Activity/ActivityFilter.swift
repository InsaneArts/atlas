/// Which rows the Activity tab lists: every one, an agent's, the person's own, or those that did not work.
public enum ActivityFilter: Sendable, CaseIterable {
  /// Every row.
  case all
  /// What an agent did.
  case agent
  /// What the person did, through a viewer.
  case you
  /// What did not work.
  case problems

  /// Whether the filter lists `entry`.
  ///
  /// - Parameter entry: A row of the log.
  /// - Returns: Whether it passes: an agent's row under Agent, the person's under You, a failed one under Problems.
  func admits(_ entry: ActivityLog.Entry) -> Bool {
    switch self {
    case .all: true
    case .agent: entry.actor == .agent
    case .you: entry.actor == .you
    case .problems: entry.outcome == .failed
    }
  }
}
