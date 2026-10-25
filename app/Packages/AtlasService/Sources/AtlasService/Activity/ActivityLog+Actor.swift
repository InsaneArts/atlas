extension ActivityLog {
  /// Who did something in the helper.
  public enum Actor: Equatable, Sendable {
    /// An agent, as a row says when it names no one.
    case agent
    /// The person, through a viewer.
    case you
    /// Anyone the service names that this app does not know.
    case someoneElse

    /// The actor the service's `actor` word names.
    ///
    /// - Parameter word: The row's `actor`: "agent", "viewer", another word, or nil when it names no one.
    init(word: String?) {
      switch word {
      case nil, "agent": self = .agent
      case "viewer": self = .you
      default: self = .someoneElse
      }
    }
  }
}
