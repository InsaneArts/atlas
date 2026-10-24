/// The panel's row of buttons, the actions on the agent (v3 P2): Stop while an agent works, then the one main action
/// filling the row. The picture's pause sits on the picture and ⋯ in the header.
public struct PanelControls: Equatable, Sendable {
  /// Whether Stop stands before the main action: only while an agent works.
  public var showsStop: Bool
  /// The one thing to do next, in white.
  public var main: PanelMainButton

  /// Creates the row.
  ///
  /// - Parameters:
  ///   - showsStop: Whether Stop stands before the main action.
  ///   - main: The one thing to do next.
  public init(showsStop: Bool = false, main: PanelMainButton) {
    self.showsStop = showsStop
    self.main = main
  }
}
