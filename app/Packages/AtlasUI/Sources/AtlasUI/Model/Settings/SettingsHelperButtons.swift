/// Which of the Helper pane's buttons can run now, each waiting while its command runs from any window, and Switch's
/// tooltip.
public struct SettingsHelperButtons: Equatable, Sendable {
  /// Whether Switch to the Helper… can run.
  public var canSwitch: Bool
  /// Whether Check Again can run: no reading it asked for is under way.
  public var canCheck: Bool
  /// Whether Recover… can run.
  public var canRecover: Bool
  /// Whether Log Out… can run.
  public var canLogOut: Bool
  /// Switch to the Helper…'s tooltip, the same in every window.
  public var switchHelp: String

  /// Creates the buttons' state.
  ///
  /// - Parameters:
  ///   - canSwitch: Whether Switch to the Helper… can run.
  ///   - canCheck: Whether Check Again can run.
  ///   - canRecover: Whether Recover… can run.
  ///   - canLogOut: Whether Log Out… can run.
  ///   - switchHelp: Switch to the Helper…'s tooltip.
  public init(canSwitch: Bool, canCheck: Bool, canRecover: Bool, canLogOut: Bool, switchHelp: String) {
    self.canSwitch = canSwitch
    self.canCheck = canCheck
    self.canRecover = canRecover
    self.canLogOut = canLogOut
    self.switchHelp = switchHelp
  }
}
