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
      operation: operation(of: request),
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

  /// What the window asks for `request`.
  ///
  /// - Parameter request: The request as the command line read it.
  /// - Returns: The operation the window shows.
  static func operation(of request: LifecycleRequest) -> ApprovalOperation {
    switch request.kind {
    case .add: .add
    case .remove: .remove
    case .uninstall: request.cancel ? .cancelUninstall : .uninstall
    case .maintain: request.end ? .endMaintenance : .maintain(restart: request.restart)
    }
  }
}
