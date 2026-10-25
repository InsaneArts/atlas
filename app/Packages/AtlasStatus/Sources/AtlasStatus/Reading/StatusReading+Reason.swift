import AtlasService

extension StatusReading {
  /// Why the helper is in its mode, or, while idle, what an agent did last: the sidebar's line, which the app words.
  public enum Reason: Equatable, Sendable {
    /// This viewer holds control.
    case yourControl
    /// Another viewer holds control.
    case inUseElsewhere
    /// Idle, and no agent has acted yet.
    case nothingHappening
    /// Idle, after an agent's newest action.
    case lastAction(ActivityItem)
    /// The helper is not ready: its first failed check, whose step may name a command.
    case failedCheck(ViewerStatus.Check)
    /// The service's own reason.
    case service(String)
    /// The service has given no reason yet.
    case connecting
  }
}
