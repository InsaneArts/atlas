extension ReadingPace {
  /// A time the window asked to read at, by why.
  public enum Deadline: Hashable, Sendable {
    /// The agents are asked again.
    case agentsRetry
    /// A job that answered nothing stops counting as restarting.
    case restartGrace
    /// This login has been on screen long enough for the check to start.
    case settled
  }
}
