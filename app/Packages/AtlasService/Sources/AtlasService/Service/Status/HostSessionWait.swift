/// What a waiting host session waits for.
public enum HostSessionWait: String, Equatable, Sendable {
  /// Its turn with the helper, which another session has.
  case helper
  /// The person, to recover an action that may not have finished.
  case recovery
}
