/// Whose rows an Activity pane lists: every row, or one host session's.
public enum ActivityScope: Hashable, Sendable {
  /// Every row.
  case all
  /// One host session's rows, by its key; the empty key is Terminal's, the agents' rows no host named.
  ///
  /// - Parameter key: The session's key.
  case session(String)

  /// Whether the scope lists `entry`: every row under All; under a session, only its agents' rows, never the
  /// person's own.
  ///
  /// - Parameter entry: A row of the log.
  /// - Returns: Whether it is in scope.
  func admits(_ entry: ActivityLog.Entry) -> Bool {
    switch self {
    case .all: true
    case .session(let key): entry.groupKey == key
    }
  }
}
