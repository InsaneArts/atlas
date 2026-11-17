/// What the person is asked to allow: adding a VM, removing one, or uninstalling Atlas with its VMs.
public enum ApprovalOperation: Hashable, Sendable {
  /// Add a VM desktop.
  case add
  /// Remove a VM desktop, deleting it.
  case remove
  /// Uninstall Atlas, deleting every VM it keeps.
  case uninstall

  /// Whether allowing it deletes something, so the window asks as a destructive alert does: Don't Allow is the
  /// default, and the action is red.
  public var isDestructive: Bool {
    switch self {
    case .add: false
    case .remove, .uninstall: true
    }
  }
}
