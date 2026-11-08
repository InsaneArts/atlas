/// The Helper pane's home folder row: private from the helper, with Undo…, or open to it, with Make Private.
public struct SettingsFilesRow: Equatable, Sendable {
  /// Whether the home folder is private from the helper.
  public var isPrivate: Bool
  /// What the row says, such as "Your home folder is private from the helper".
  public var title: String
  /// The line under it, such as "Such as dev and .config".
  public var detail: String
  /// The button's title: Undo… or Make Private.
  public var button: String
  /// Whether the button takes presses now; it waits while a change runs.
  public var canRun: Bool

  /// Creates the row.
  ///
  /// - Parameters:
  ///   - isPrivate: Whether the home folder is private from the helper.
  ///   - title: What the row says.
  ///   - detail: The line under it.
  ///   - button: The button's title.
  ///   - canRun: Whether the button takes presses now.
  public init(isPrivate: Bool, title: String, detail: String, button: String, canRun: Bool) {
    self.isPrivate = isPrivate
    self.title = title
    self.detail = detail
    self.button = button
    self.canRun = canRun
  }
}
