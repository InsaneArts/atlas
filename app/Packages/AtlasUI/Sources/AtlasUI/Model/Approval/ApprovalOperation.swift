/// What the person is asked to allow: adding a VM, removing one, uninstalling Atlas with its VMs, calling an
/// uninstall under way off, or a VM's maintenance begun or ended.
public enum ApprovalOperation: Hashable, Sendable {
  /// Add a VM desktop.
  case add
  /// Remove a VM desktop, deleting it.
  case remove
  /// Uninstall Atlas, deleting every VM it keeps.
  case uninstall
  /// Call off an uninstall under way: nothing more is removed.
  case cancelUninstall
  /// Open a VM's window for the person's maintenance, after stopping it and starting it again when `restart`.
  case maintain(restart: Bool)
  /// End a VM's maintenance: its window closes.
  case endMaintenance

  /// Whether allowing it deletes something, so the window asks as a destructive alert does: Don't Allow is the
  /// default, and the action is red.
  public var isDestructive: Bool {
    switch self {
    case .add, .cancelUninstall, .endMaintenance: false
    case .remove, .uninstall: true
    case .maintain(let restart): restart
    }
  }
}
