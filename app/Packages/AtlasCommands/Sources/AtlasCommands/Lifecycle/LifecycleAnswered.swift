/// What became of a request the person answered already: nothing is left to answer, whatever its time.
public enum LifecycleAnswered: String, Decodable, Equatable, Sendable {
  /// Allowed, its grant not used yet.
  case allowed
  /// Refused.
  case refused
  /// Allowed, and its grant used.
  case used
}
