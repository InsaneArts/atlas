/// The shortcut a menu item draws beside it: one key with Command, and Option, Control or Shift as it says. It is the
/// main menu's own shortcut for the same command, drawn so the menu teaches it.
public struct ViewerMenuShortcut: Equatable, Sendable {
  /// The key, such as "r".
  public var key: Character
  /// Whether Option is held too.
  public var option: Bool
  /// Whether Control is held too.
  public var control: Bool
  /// Whether Shift is held too.
  public var shift: Bool

  /// Creates a shortcut with Command.
  ///
  /// - Parameters:
  ///   - key: The key.
  ///   - option: Whether Option is held too; not by default.
  ///   - control: Whether Control is held too; not by default.
  ///   - shift: Whether Shift is held too; not by default.
  public init(_ key: Character, option: Bool = false, control: Bool = false, shift: Bool = false) {
    self.key = key
    self.option = option
    self.control = control
    self.shift = shift
  }
}
