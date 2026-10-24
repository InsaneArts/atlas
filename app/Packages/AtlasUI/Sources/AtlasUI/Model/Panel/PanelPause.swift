/// The pause button on the panel's picture: whether the picture is paused, and what the button says for
/// that. It pauses only the picture; work in the helper goes on.
public struct PanelPause: Equatable, Sendable {
  /// Whether the picture is paused, which turns the button's pause symbol into play.
  public var isPaused: Bool
  /// What VoiceOver says for it.
  public var label: String
  /// Its tooltip.
  public var help: String

  /// Creates the button's state.
  ///
  /// - Parameters:
  ///   - isPaused: Whether the picture is paused.
  ///   - label: What VoiceOver says for it.
  ///   - help: Its tooltip.
  public init(isPaused: Bool, label: String, help: String) {
    self.isPaused = isPaused
    self.label = label
    self.help = help
  }
}
