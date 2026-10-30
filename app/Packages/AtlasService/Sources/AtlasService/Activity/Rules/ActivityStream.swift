/// Whose work a moment of the Activity pane follows: one host session's agents, or the person. Each has its own
/// stream, so two agents working at once never cut each other's moments into pieces.
public enum ActivityStream: Hashable, Sendable {
  /// The person, through a viewer.
  case you
  /// One host session's agents, by its key; the empty key is Terminal's, the agents' rows no host named.
  ///
  /// - Parameter key: The session's key.
  case session(String)

  /// The stream a row belongs to: the person's own, else its host session's.
  ///
  /// - Parameter entry: A row of the log.
  init(_ entry: ActivityLog.Entry) {
    self = entry.groupKey.map(Self.session) ?? .you
  }

  /// The stream's name in a moment's id: `you`, or `session:` and its key.
  public var id: String {
    switch self {
    case .you: "you"
    case .session(let key): "session:\(key)"
    }
  }
}
