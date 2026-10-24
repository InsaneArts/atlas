/// The panel's row of buttons, left to right: Stop while an agent works, the one main action filling the row, the
/// pause button and ⋯.
public struct PanelControls: Equatable, Sendable {
  /// Whether Stop stands before the main action: only while an agent works.
  public var showsStop: Bool
  /// The one thing to do next, in white.
  public var main: PanelMainButton
  /// The pause button: whether the panel's picture is paused, and its words.
  public var pause: PanelPause
  /// Whether the ⋯ menu is open, which draws its button pressed.
  public var isMoreOpen: Bool

  /// Creates the row.
  ///
  /// - Parameters:
  ///   - showsStop: Whether Stop stands before the main action.
  ///   - main: The one thing to do next.
  ///   - pause: The pause button's state and words.
  ///   - isMoreOpen: Whether the ⋯ menu is open.
  public init(showsStop: Bool = false, main: PanelMainButton, pause: PanelPause, isMoreOpen: Bool = false) {
    self.showsStop = showsStop
    self.main = main
    self.pause = pause
    self.isMoreOpen = isMoreOpen
  }
}
