/// The Privacy page's home folder row: private from the helper, with Undo…, or open to it, with Make Private.
public struct SettingsFilesRow: Equatable, Sendable, Identifiable {
  /// Whether the home folder is private from the helper.
  public var isPrivate: Bool
  /// What the row says, such as "Your home folder is private from the helper".
  public var title: String
  /// The line under it, such as "Such as dev and .config".
  public var detail: String
  /// The button's title: Undo… or Make Private.
  public var button: String
  /// The longest title the button shows, either way, which holds its width as the title changes.
  public var heldButton: String
  /// Whether the button takes presses now; it waits while a change runs.
  public var canRun: Bool
  /// Whether this row's change runs now, which shows a spinner in the button's title's place.
  public var isWorking: Bool

  /// Creates the row.
  ///
  /// - Parameters:
  ///   - isPrivate: Whether the home folder is private from the helper.
  ///   - title: What the row says.
  ///   - detail: The line under it.
  ///   - button: The button's title.
  ///   - heldButton: The longest title the button shows; `button` by default.
  ///   - canRun: Whether the button takes presses now.
  ///   - isWorking: Whether this row's change runs now; not by default.
  public init(
    isPrivate: Bool,
    title: String,
    detail: String,
    button: String,
    heldButton: String? = nil,
    canRun: Bool,
    isWorking: Bool = false
  ) {
    self.isPrivate = isPrivate
    self.title = title
    self.detail = detail
    self.button = button
    self.heldButton = heldButton ?? button
    self.canRun = canRun
    self.isWorking = isWorking
  }

  /// The page holds one home folder row, whose words change in place.
  public var id: String { "home" }
}
