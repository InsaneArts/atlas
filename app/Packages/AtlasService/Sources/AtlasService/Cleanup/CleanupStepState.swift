/// Where one step of a cleanup stands once it has been run, as the service names it. A word this app does not know
/// reads as ``unknown``.
public enum CleanupStepState: String, Equatable, Sendable {
  /// It has not started yet.
  case pending
  /// It started, and has not ended.
  case started
  /// It worked, or what it removes was already gone.
  case done
  /// It was left as it is on purpose, as for an app that changed since it was installed.
  case kept
  /// It did not work.
  case failed
  /// It did not run, as agents are stopped.
  case blocked
  /// Whether it worked is not known.
  case uncertain
  /// A state this app does not know.
  case unknown

  /// The state the service's word names, or nil when the step has none.
  ///
  /// - Parameter word: The step's `state`, or nil when it has none, as a step not run yet has none.
  init?(word: String?) {
    guard let word, !word.isEmpty else { return nil }
    self = Self(rawValue: word) ?? .unknown
  }
}
