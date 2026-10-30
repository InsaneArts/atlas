/// The panel's row of buttons, the actions on the agent: Stop while an agent works, or Resume once the person stopped
/// agents, in a secondary slot, then the one main action filling the row. The picture's pause sits on the picture and
/// ⋯ in the header.
public struct PanelControls: Equatable, Sendable {
  /// The secondary button before the main action: Stop while an agent works, Resume once agents are stopped; nil shows
  /// none.
  public var agent: ViewerAgentButton?
  /// Stop's or Resume's tooltip: the menu item's title and its shortcut, as the main window's button says it.
  public var agentHelp: String
  /// The one thing to do next, in white.
  public var main: PanelMainButton

  /// Creates the row.
  ///
  /// - Parameters:
  ///   - agent: Stop or Resume before the main action; none by default.
  ///   - agentHelp: Its tooltip; none by default.
  ///   - main: The one thing to do next.
  public init(agent: ViewerAgentButton? = nil, agentHelp: String = "", main: PanelMainButton) {
    self.agent = agent
    self.agentHelp = agentHelp
    self.main = main
  }
}
