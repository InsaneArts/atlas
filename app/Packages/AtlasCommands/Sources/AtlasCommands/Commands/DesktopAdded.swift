/// What `atlas desktop add vm` reports once the VM is added: its name in the settings.
public struct DesktopAdded: Decodable, Sendable {
  /// The desktop added.
  public struct Added: Decodable, Sendable {
    /// Its name in the settings, such as `vm-2`.
    public let name: String?
  }

  /// The desktop added; nil when the reply names none.
  public let added: Added?

  /// The name of the desktop added; nil when the reply names none.
  public var name: String? { added?.name.flatMap { $0.isEmpty ? nil : $0 } }
}
