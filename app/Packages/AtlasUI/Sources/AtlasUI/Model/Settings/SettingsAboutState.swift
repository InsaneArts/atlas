/// The About pane: the app's version, its update button and what the last check said.
public struct SettingsAboutState: Equatable, Sendable {
  /// The version, such as "Version 0.1.0".
  public var version: String
  /// The main button.
  public var update: SettingsUpdateButton
  /// What the last check or install said, under the buttons; nil for nothing.
  public var answer: String?

  /// Creates the pane's state.
  ///
  /// - Parameters:
  ///   - version: The version.
  ///   - update: The main button.
  ///   - answer: What the last check or install said.
  public init(version: String, update: SettingsUpdateButton, answer: String? = nil) {
    self.version = version
    self.update = update
    self.answer = answer
  }
}
