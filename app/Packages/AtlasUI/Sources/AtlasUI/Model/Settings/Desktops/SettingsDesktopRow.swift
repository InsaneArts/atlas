/// One desktop in the Desktops list: its kind's symbol, its title with a quiet "Default" on the default one, and what
/// it is with how it stands, such as "A virtual Mac on this Mac · Running".
public struct SettingsDesktopRow: Equatable, Sendable, Identifiable {
  /// The desktop's ID, which its page is routed by.
  public var id: String
  /// Its title, such as "Second Login" or "work".
  public var title: String
  /// What it is, with how it stands.
  public var detail: String
  /// Whether it is the settings' default, which its quiet tag says.
  public var isDefault: Bool
  /// Whether it is a VM, which its symbol shows: a window, else a person.
  public var isVM: Bool

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - id: The desktop's ID.
  ///   - title: Its title.
  ///   - detail: What it is, with how it stands.
  ///   - isDefault: Whether it is the default; not by default.
  ///   - isVM: Whether it is a VM.
  public init(id: String, title: String, detail: String, isDefault: Bool = false, isVM: Bool) {
    self.id = id
    self.title = title
    self.detail = detail
    self.isDefault = isDefault
    self.isVM = isVM
  }
}
