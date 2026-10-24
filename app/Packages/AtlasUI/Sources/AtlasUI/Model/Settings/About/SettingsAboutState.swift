/// The About pane: the app's version, its one update button and what it says about updates.
public struct SettingsAboutState: Equatable, Sendable {
  /// The version, such as "Version 0.1.0".
  public var version: String
  /// The update button, whatever the check's state.
  public var update: SettingsUpdateButton
  /// What the last check or install says, under the buttons; nil for nothing.
  public var status: SettingsUpdateStatus?

  /// Creates the pane's state.
  ///
  /// - Parameters:
  ///   - version: The version.
  ///   - update: The update button.
  ///   - status: What the last check or install says.
  public init(version: String, update: SettingsUpdateButton, status: SettingsUpdateStatus? = nil) {
    self.version = version
    self.update = update
    self.status = status
  }
}
