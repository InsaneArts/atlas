/// One account in the Accounts pane: its first letter on a disc, its name, when it was added, and how freely agents
/// act there.
public struct SettingsAccountRow: Equatable, Sendable, Identifiable {
  /// The account, as the service names it: its kind, key and browser together.
  public var id: String
  /// The first letter of its name, on its disc.
  public var letter: String
  /// Its name, such as `github.com` or `Slack`.
  public var title: String
  /// When it was added, such as "Added today" or "App · added Nov 5".
  public var detail: String
  /// How freely agents act there.
  public var level: AccountLevelChoice

  /// Creates an account's row.
  ///
  /// - Parameters:
  ///   - id: The account, as the service names it.
  ///   - letter: The first letter of its name.
  ///   - title: Its name.
  ///   - detail: When it was added.
  ///   - level: How freely agents act there; Ask Before Sending by default.
  public init(id: String, letter: String, title: String, detail: String, level: AccountLevelChoice = .ask) {
    self.id = id
    self.letter = letter
    self.title = title
    self.detail = detail
    self.level = level
  }
}
