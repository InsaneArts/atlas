extension StatusReading {
  /// Why a viewer that does not hold control must wait to take it.
  package enum ControlBlock: Equatable, Sendable {
    /// Recovery must run first.
    case recovery
    /// Another viewer holds control.
    case anotherWindow
  }
}
