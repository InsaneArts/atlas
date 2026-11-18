/// What a request asks the person to allow: adding a VM, removing one, uninstalling Atlas with its VMs, or a VM's
/// maintenance.
public enum LifecycleKind: String, Decodable, Equatable, Sendable {
  /// Add a VM desktop.
  case add
  /// Remove a VM desktop.
  case remove
  /// Uninstall Atlas, removing every VM it keeps.
  case uninstall
  /// Open a VM's window for the person's maintenance, or end it.
  case maintain
}
