/// One of the top row's round toggles, such as the picture's pause or Details: whether it is on, and the words that
/// say what pressing it does.
package struct ViewerToggle: Equatable, Sendable {
  /// Whether it is on: the picture paused, Details showing, or its menu open.
  package var isOn: Bool
  /// What VoiceOver calls it: what pressing it does.
  package var label: String
  /// Its tooltip: what pressing it does, with its shortcut.
  package var help: String

  /// Creates a toggle's state.
  ///
  /// - Parameters:
  ///   - isOn: Whether it is on.
  ///   - label: What VoiceOver calls it: what pressing it does.
  ///   - help: Its tooltip: what pressing it does, with its shortcut.
  package init(isOn: Bool, label: String, help: String) {
    self.isOn = isOn
    self.label = label
    self.help = help
  }
}
