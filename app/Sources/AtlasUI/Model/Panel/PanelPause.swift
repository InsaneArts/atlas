/// The panel's pause button: whether its picture is paused, and what the button says for that.
package struct PanelPause: Equatable, Sendable {
  /// Whether the picture is paused, which draws the button pressed with a play symbol.
  package var isPaused: Bool
  /// What VoiceOver says for it.
  package var label: String
  /// Its tooltip.
  package var help: String

  /// Creates the button's state.
  ///
  /// - Parameters:
  ///   - isPaused: Whether the picture is paused.
  ///   - label: What VoiceOver says for it.
  ///   - help: Its tooltip.
  package init(isPaused: Bool, label: String, help: String) {
    self.isPaused = isPaused
    self.label = label
    self.help = help
  }
}
