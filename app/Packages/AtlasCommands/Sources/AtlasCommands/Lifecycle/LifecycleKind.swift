/// What a request asks the person to allow: adding a VM, removing one, or uninstalling Atlas with its VMs.
public enum LifecycleKind: String, Decodable, Equatable, Sendable {
  /// Add a VM desktop.
  case add
  /// Remove a VM desktop.
  case remove
  /// Uninstall Atlas, removing every VM it keeps.
  case uninstall
}
