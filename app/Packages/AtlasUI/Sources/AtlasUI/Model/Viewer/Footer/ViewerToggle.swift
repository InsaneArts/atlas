/// One of the window's round toggles, such as the picture's pause: whether it is on, and the words that say what
/// pressing it does.
public struct ViewerToggle: Equatable, Sendable {
  /// Whether it is on: the picture paused, say.
  public var isOn: Bool
  /// What VoiceOver calls it: what pressing it does.
  public var label: String
  /// Its tooltip: what pressing it does, with its shortcut.
  public var help: String

  /// Creates a toggle's state.
  ///
  /// - Parameters:
  ///   - isOn: Whether it is on.
  ///   - label: What VoiceOver calls it: what pressing it does.
  ///   - help: Its tooltip: what pressing it does, with its shortcut.
  public init(isOn: Bool, label: String, help: String) {
    self.isOn = isOn
    self.label = label
    self.help = help
  }
}
