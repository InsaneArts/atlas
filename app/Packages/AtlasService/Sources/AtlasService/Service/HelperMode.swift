/// What the helper is doing, as the service names it. A name this app does
/// not know reads as unavailable.
public enum HelperMode: String, Decodable, Sendable {
  /// Nothing is acting; the helper is watched.
  case observing
  /// An agent is acting in the helper.
  case agent
  /// A viewer holds mouse and keyboard control.
  case controlling
  /// An action's outcome is unknown; recovery must run first.
  case recovering
  /// The person stopped agents; they wait until the person resumes them.
  case stopped
  /// The helper is not ready for actions.
  case blocked
  /// The service cannot reach the helper.
  case unavailable
  /// The helper's account is logged out.
  case loggedOut

  /// The helper answers: control, readiness steps and recovery apply.
  public var answers: Bool { self != .unavailable && self != .loggedOut }
}
