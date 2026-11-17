import AtlasCommands
import AtlasUI

/// Builds the approval window's state from what its controller holds. Pure.
enum ApprovalPresenter {
  /// The window as it should read now: what the request asks, for which VM, from which session, and where the
  /// answer stands.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The window's state.
  static func state(_ snapshot: ApprovalSnapshot) -> ApprovalState {
    guard let request = snapshot.request else {
      return ApprovalState(operation: nil, desktop: "", session: "", phase: snapshot.phase)
    }
    return ApprovalState(
      operation: operation(of: request.kind),
      desktop: request.name,
      session: request.session,
      phase: snapshot.phase
    )
  }

  /// What the window says of an answer given before it was opened again.
  ///
  /// - Parameter answered: The answer the command line kept.
  /// - Returns: The answer the window shows.
  static func earlier(_ answered: LifecycleAnswered) -> ApprovalEarlierAnswer {
    switch answered {
    case .allowed: .allowed
    case .refused: .refused
    case .used: .used
    }
  }

  /// What the window asks for a request of `kind`.
  ///
  /// - Parameter kind: The request's kind.
  /// - Returns: The operation the window shows.
  static func operation(of kind: LifecycleKind) -> ApprovalOperation {
    switch kind {
    case .add: .add
    case .remove: .remove
    case .uninstall: .uninstall
    }
  }
}
