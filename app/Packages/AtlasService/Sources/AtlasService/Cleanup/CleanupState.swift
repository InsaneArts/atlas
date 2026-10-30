/// How far a cleanup of a session's work got, as the service names it. A word this app does not know reads as
/// ``unknown``.
public enum CleanupState: String, Equatable, Sendable {
  /// Every step is done, or kept on purpose.
  case complete
  /// A step failed, so some of the work is still there.
  case incomplete
  /// A step did not run, as agents are stopped.
  case blocked
  /// Whether a step worked is not known.
  case uncertain
  /// A state this app does not know.
  case unknown

  /// The state the service's word names, or nil when it wrote none.
  ///
  /// - Parameter word: The `state` the service wrote, or nil when it wrote none.
  init?(word: String?) {
    guard let word, !word.isEmpty else { return nil }
    self = Self(rawValue: word) ?? .unknown
  }
}
