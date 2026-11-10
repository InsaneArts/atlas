/// An account's own page, pushed over Accounts: its letter, name and what it is, how freely agents act there with what
/// that level lets them do, for a site its sign-in's actions, and Remove from List. While Atlas can't read its list the
/// page keeps the name it last knew and says so, its choices waiting.
public struct SettingsAccountPageState: Equatable, Sendable {
  /// The account, as the service names it.
  public var id: String
  /// The first letter of its name, on its disc.
  public var letter: String
  /// Its name, such as `github.com` or `Slack`.
  public var title: String
  /// What it is and when it was added, such as "Website · added today" or "App · added Nov 5".
  public var kindLine: String
  /// Whether it is a site, which Sign Out…, Forget… and Reset… act on.
  public var isSite: Bool
  /// How freely agents act there; nil while Atlas can't read its list.
  public var level: AccountLevelChoice?
  /// What the level lets agents do there, such as "Agents ask you before sending there."; empty while unknown.
  public var levelLine: String
  /// Whether its choices take presses now: the list can be read and no change runs.
  public var isEnabled: Bool
  /// What the last change on the page said; nil for nothing.
  public var notice: SettingsNote?
  /// Its app's or site's icon; nil draws its first letter.
  public var icon: SettingsAccountIcon?

  /// Creates the page's state.
  ///
  /// - Parameters:
  ///   - id: The account, as the service names it.
  ///   - letter: The first letter of its name.
  ///   - title: Its name.
  ///   - kindLine: What it is and when it was added.
  ///   - isSite: Whether it is a site.
  ///   - level: How freely agents act there; nil while unknown.
  ///   - levelLine: What the level lets agents do there.
  ///   - isEnabled: Whether its choices take presses now.
  ///   - notice: What the last change on the page said; none by default.
  ///   - icon: Its app's or site's icon; none by default.
  public init(
    id: String,
    letter: String,
    title: String,
    kindLine: String,
    isSite: Bool,
    level: AccountLevelChoice?,
    levelLine: String,
    isEnabled: Bool,
    notice: SettingsNote? = nil,
    icon: SettingsAccountIcon? = nil
  ) {
    self.id = id
    self.letter = letter
    self.title = title
    self.kindLine = kindLine
    self.isSite = isSite
    self.level = level
    self.levelLine = levelLine
    self.isEnabled = isEnabled
    self.notice = notice
    self.icon = icon
  }

  /// Whether Atlas can't read its list now, so the page says so and its choices wait.
  public var isUnreadable: Bool { level == nil }
}
