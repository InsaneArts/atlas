/// What a host session is doing, as the service decides it for every app at once. A word this app does not know
/// reads as ``idle``.
public enum HostSessionState: String, Equatable, Sendable {
  /// It has the helper now.
  case acting
  /// It waits for the helper, or for the person to recover it.
  case waiting
  /// Its action may not have finished, and the helper needs the person.
  case needsYou
  /// Nothing of the above.
  case idle
  /// It has ended, or went quiet long ago.
  case ended

  /// The state the service's word names.
  ///
  /// - Parameter word: The session's `state`, or nil when it names none.
  init(word: String?) {
    self = word.flatMap(Self.init(rawValue:)) ?? .idle
  }
}
