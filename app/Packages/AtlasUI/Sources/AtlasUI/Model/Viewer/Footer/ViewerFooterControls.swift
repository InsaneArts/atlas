/// The footer's buttons at its trailing edge, in the order they stand: the picture's pause, Stop or Resume, and the main
/// capsule. Only the actions that apply are drawn; each comes and goes in its slot, and the main capsule's trailing edge
/// never moves.
public struct ViewerFooterControls: Equatable, Sendable {
  /// The picture's pause, on while the picture is paused; nil while there is no picture to pause, or while this window
  /// holds control, where pausing the picture would leave the person working blind.
  public var pause: ViewerToggle?
  /// Stop while an agent works, Resume once stopped; nil shows neither.
  public var agent: ViewerAgentButton?
  /// Stop's or Resume's tooltip: its menu item and shortcut; empty while neither shows.
  public var agentHelp: String
  /// Take Control, Give Back or Recover…; nil while the helper is logged out, not ready or still connecting, where the
  /// stage offers what to do instead.
  public var control: ViewerControlButton?

  /// Creates the footer's buttons.
  ///
  /// - Parameters:
  ///   - pause: The picture's pause; nil hides it.
  ///   - agent: Stop or Resume; nil shows neither.
  ///   - agentHelp: Stop's or Resume's tooltip, with its shortcut.
  ///   - control: Take Control, Give Back or Recover…; nil hides it.
  public init(
    pause: ViewerToggle?,
    agent: ViewerAgentButton?,
    agentHelp: String = "",
    control: ViewerControlButton?
  ) {
    self.pause = pause
    self.agent = agent
    self.agentHelp = agentHelp
    self.control = control
  }
}
