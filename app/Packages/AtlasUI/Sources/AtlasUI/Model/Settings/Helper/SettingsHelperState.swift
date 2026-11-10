/// The Helper pane: the helper's account, whether it is ready to act, and what to do when something goes wrong.
public struct SettingsHelperState: Equatable, Sendable {
  /// The helper's account, with its status.
  public var account: SettingsHelperAccount
  /// A login Atlas can close, listed first while there is one; nil for none.
  public var closeLogin: SettingsCloseLoginRow?
  /// Each readiness check; what to do shows under a check only while it is not ready.
  public var ready: [SettingsReadyRow]
  /// Which buttons can run now, and Switch's tooltip.
  public var buttons: SettingsHelperButtons
  /// Why the buttons wait, while another action runs from any window, naming it, such as "Recover is running. Try
  /// again once it ends."; nil while nothing runs.
  public var waiting: String?
  /// What Check Again found when something isn't ready, under "Ready to act"; nil for nothing, as when every check
  /// passed, which the green marks say.
  public var readyNote: SettingsNote?
  /// What the last command said, under the pane's last group; nil for nothing.
  public var notice: SettingsNote?

  /// Creates the pane's state.
  ///
  /// - Parameters:
  ///   - account: The helper's account.
  ///   - closeLogin: A login Atlas can close; nil by default.
  ///   - ready: Each readiness check.
  ///   - buttons: Which buttons can run now.
  ///   - waiting: Why the buttons wait, while another action runs.
  ///   - readyNote: What Check Again found when something isn't ready.
  ///   - notice: What the last command said.
  public init(
    account: SettingsHelperAccount,
    closeLogin: SettingsCloseLoginRow? = nil,
    ready: [SettingsReadyRow],
    buttons: SettingsHelperButtons,
    waiting: String? = nil,
    readyNote: SettingsNote? = nil,
    notice: SettingsNote? = nil
  ) {
    self.account = account
    self.closeLogin = closeLogin
    self.ready = ready
    self.buttons = buttons
    self.waiting = waiting
    self.readyNote = readyNote
    self.notice = notice
  }
}
