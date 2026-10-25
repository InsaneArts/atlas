extension SessionFacts {
  /// Whether this viewer could take control now, and if not, why.
  enum Control: Equatable, Sendable {
    /// This viewer holds control.
    case thisWindow
    /// Control can be taken: the helper is watched, stopped or used by an agent.
    case available
    /// Another viewer holds control.
    case anotherWindow
    /// Recovery must run first.
    case afterRecovery
    /// The helper is not ready.
    case notReady
    /// The helper is logged out.
    case loggedOut
  }
}
