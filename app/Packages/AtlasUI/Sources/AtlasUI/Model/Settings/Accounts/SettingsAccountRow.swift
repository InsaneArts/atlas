/// One account in the Accounts pane: its app's or site's icon, else its first letter on a tile, its name, when it
/// was added, and how freely agents act there.
public struct SettingsAccountRow: Equatable, Sendable, Identifiable {
  /// The account, as the service names it: its kind, key and browser together.
  public var id: String
  /// The first letter of its name, on its disc.
  public var letter: String
  /// Its name, such as `github.com` or `Slack`.
  public var title: String
  /// Its level and when it was added, such as "Ask before sending · added today" or "Trusted · App · added Nov 5".
  public var detail: String
  /// How freely agents act there.
  public var level: AccountLevelChoice
  /// Whether it is a site, which Sign In…, Forget This Site… and Reset the Helper's Browser… act on.
  public var isSite: Bool
  /// Its app's or site's icon; nil draws its first letter.
  public var icon: SettingsAccountIcon?

  /// Creates an account's row.
  ///
  /// - Parameters:
  ///   - id: The account, as the service names it.
  ///   - letter: The first letter of its name.
  ///   - title: Its name.
  ///   - detail: When it was added.
  ///   - level: How freely agents act there; Ask Before Sending by default.
  ///   - isSite: Whether it is a site; not by default.
  ///   - icon: Its app's or site's icon; none by default.
  public init(
    id: String,
    letter: String,
    title: String,
    detail: String,
    level: AccountLevelChoice = .ask,
    isSite: Bool = false,
    icon: SettingsAccountIcon? = nil
  ) {
    self.id = id
    self.letter = letter
    self.title = title
    self.detail = detail
    self.level = level
    self.isSite = isSite
    self.icon = icon
  }
}
