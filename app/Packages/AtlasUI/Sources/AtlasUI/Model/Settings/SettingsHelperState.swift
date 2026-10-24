/// The Helper pane: the helper's account with its status, whether it is ready to act, and what to do when something
/// goes wrong.
public struct SettingsHelperState: Equatable, Sendable {
  /// The helper as people know it, such as "Atlas".
  public var account: String
  /// The helper's status, whose mark leads the account and whose word VoiceOver reads.
  public var status: StatusSummary
  /// What the account is, such as "The hidden helper login · 2560 × 1440".
  public var line: String
  /// Each readiness check; what to do shows under a check only while it is not ready.
  public var ready: [SettingsReadyRow]
  /// Which buttons can run now, and Switch's tooltip.
  public var buttons: SettingsHelperButtons
  /// What the last command said, under the pane's last group; nil for nothing.
  public var notice: SettingsNote?

  /// Creates the pane's state.
  ///
  /// - Parameters:
  ///   - account: The helper as people know it.
  ///   - status: The helper's status.
  ///   - line: What the account is.
  ///   - ready: Each readiness check.
  ///   - buttons: Which buttons can run now.
  ///   - notice: What the last command said.
  public init(
    account: String,
    status: StatusSummary,
    line: String,
    ready: [SettingsReadyRow],
    buttons: SettingsHelperButtons,
    notice: SettingsNote? = nil
  ) {
    self.account = account
    self.status = status
    self.line = line
    self.ready = ready
    self.buttons = buttons
    self.notice = notice
  }
}
