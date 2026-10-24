/// The top row's buttons on the right, in the order they stand (v3 D11): Clear, pause, Keys, Stop or Resume, the white
/// capsule, ⋯ and Details. Only the actions that apply are drawn; each comes and goes in its slot.
public struct ViewerTopActions: Equatable, Sendable {
  /// Whether Clear shows: a window from the Apps tab is outlined on the screen.
  public var showsClear: Bool
  /// The picture's pause, on while the picture is paused; nil while there is no picture to pause.
  public var pause: ViewerToggle?
  /// Keys: shown only while this window holds control.
  public var keys: ViewerKeysButton
  /// Stop while an agent works, Resume once stopped; nil shows neither.
  public var agent: ViewerAgentButton?
  /// Stop's or Resume's tooltip: what it does, with its shortcut; empty while neither shows.
  public var agentHelp: String
  /// Take Control, Give Back or Recover…; nil while the helper is logged out, not ready or still connecting, where the
  /// stage offers what to do instead.
  public var control: ViewerControlButton?
  /// Counts Take Control's refusals, so each one shakes the white capsule once (v3 V12).
  public var refusals: Int
  /// ⋯: on while its menu is open.
  public var more: ViewerToggle
  /// Details: on while the Details column shows.
  public var details: ViewerToggle

  /// Creates the top row's buttons.
  ///
  /// - Parameters:
  ///   - pause: The picture's pause; nil hides it.
  ///   - keys: Keys.
  ///   - agent: Stop or Resume; nil shows neither.
  ///   - agentHelp: Stop's or Resume's tooltip, with its shortcut.
  ///   - control: Take Control, Give Back or Recover…; nil hides it.
  ///   - refusals: Take Control's refusals counted; none by default.
  ///   - more: ⋯.
  ///   - details: Details.
  ///   - showsClear: Whether Clear shows; it does not by default.
  public init(
    pause: ViewerToggle?,
    keys: ViewerKeysButton,
    agent: ViewerAgentButton?,
    agentHelp: String = "",
    control: ViewerControlButton?,
    refusals: Int = 0,
    more: ViewerToggle,
    details: ViewerToggle,
    showsClear: Bool = false
  ) {
    self.pause = pause
    self.keys = keys
    self.agent = agent
    self.agentHelp = agentHelp
    self.control = control
    self.refusals = refusals
    self.more = more
    self.details = details
    self.showsClear = showsClear
  }
}
