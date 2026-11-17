/// What the person answered to a request before the window was opened again: allowed, refused, or allowed and its
/// grant used.
public enum ApprovalEarlierAnswer: Hashable, Sendable {
  /// Allowed, its grant not used yet.
  case allowed
  /// Refused.
  case refused
  /// Allowed, and its grant used.
  case used
}
