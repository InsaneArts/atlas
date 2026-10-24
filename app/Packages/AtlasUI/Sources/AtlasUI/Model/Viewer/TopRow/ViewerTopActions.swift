/// The top row's buttons on the right, in the order they stand: Clear, pause, Keys, Stop or Resume, the white capsule,
/// ⋯ and Details.
public struct ViewerTopActions: Equatable, Sendable {
  /// Whether Clear shows: a window from the Apps tab is outlined on the screen.
  public var showsClear: Bool
  /// The picture's pause: on while the picture is paused.
  public var pause: ViewerToggle
  /// Keys: pressed in control, faded otherwise, gone while the helper needs the person.
  public var keys: ViewerKeysButton
  /// Stop while an agent works, Resume once stopped; nil shows neither.
  public var agent: ViewerAgentButton?
  /// Stop's or Resume's tooltip: what it does, with its shortcut; empty while neither shows.
  public var agentHelp: String
  /// Take Control, Give Back or Recover….
  public var control: ViewerControlButton
  /// ⋯: on while its menu is open.
  public var more: ViewerToggle
  /// Details: on while the Details column shows.
  public var details: ViewerToggle

  /// Creates the top row's buttons.
  ///
  /// - Parameters:
  ///   - pause: The picture's pause.
  ///   - keys: Keys.
  ///   - agent: Stop or Resume; nil shows neither.
  ///   - agentHelp: Stop's or Resume's tooltip, with its shortcut.
  ///   - control: Take Control, Give Back or Recover….
  ///   - more: ⋯.
  ///   - details: Details.
  ///   - showsClear: Whether Clear shows; it does not by default.
  public init(
    pause: ViewerToggle,
    keys: ViewerKeysButton,
    agent: ViewerAgentButton?,
    agentHelp: String = "",
    control: ViewerControlButton,
    more: ViewerToggle,
    details: ViewerToggle,
    showsClear: Bool = false
  ) {
    self.pause = pause
    self.keys = keys
    self.agent = agent
    self.agentHelp = agentHelp
    self.control = control
    self.more = more
    self.details = details
    self.showsClear = showsClear
  }
}
