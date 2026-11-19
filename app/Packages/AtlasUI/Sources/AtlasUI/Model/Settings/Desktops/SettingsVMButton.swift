/// A small button at the end of a VM page's row: its title, what it sends, whether it is the destructive one, and
/// whether it can run now.
public struct SettingsVMButton: Equatable, Sendable {
  /// Its title, such as "Stop…".
  public var title: String
  /// What it sends.
  public var event: SettingsEvent
  /// Whether it is drawn in the destructive tint, as Remove… is.
  public var isDestructive: Bool
  /// Whether it can run now; it waits, dimmed, while it cannot.
  public var isEnabled: Bool

  /// Creates a button.
  ///
  /// - Parameters:
  ///   - title: Its title.
  ///   - event: What it sends.
  ///   - isDestructive: Whether it is drawn in the destructive tint; not by default.
  ///   - isEnabled: Whether it can run now; it can by default.
  public init(title: String, event: SettingsEvent, isDestructive: Bool = false, isEnabled: Bool = true) {
    self.title = title
    self.event = event
    self.isDestructive = isDestructive
    self.isEnabled = isEnabled
  }
}
