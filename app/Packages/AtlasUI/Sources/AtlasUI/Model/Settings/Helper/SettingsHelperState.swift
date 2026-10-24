/// The Helper pane: the helper's account, whether it is ready to act, and what to do when something goes wrong.
public struct SettingsHelperState: Equatable, Sendable {
  /// The helper's account, with its status.
  public var account: SettingsHelperAccount
  /// Each readiness check; what to do shows under a check only while it is not ready.
  public var ready: [SettingsReadyRow]
  /// Which buttons can run now, and Switch's tooltip.
  public var buttons: SettingsHelperButtons
  /// Why the buttons wait, while another action runs from any window, such as "Another action is running. Try again
  /// once it ends."; nil while nothing runs (v3 G6).
  public var waiting: String?
  /// What the last command said, under the pane's last group; nil for nothing.
  public var notice: SettingsNote?

  /// Creates the pane's state.
  ///
  /// - Parameters:
  ///   - account: The helper's account.
  ///   - ready: Each readiness check.
  ///   - buttons: Which buttons can run now.
  ///   - waiting: Why the buttons wait, while another action runs.
  ///   - notice: What the last command said.
  public init(
    account: SettingsHelperAccount,
    ready: [SettingsReadyRow],
    buttons: SettingsHelperButtons,
    waiting: String? = nil,
    notice: SettingsNote? = nil
  ) {
    self.account = account
    self.ready = ready
    self.buttons = buttons
    self.waiting = waiting
    self.notice = notice
  }
}
