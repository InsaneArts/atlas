/// How freely agents act in an account, as the person picks it in Settings › Accounts.
public enum AccountLevelChoice: String, Equatable, Hashable, Sendable, CaseIterable {
  /// Agents read, scroll and take screenshots; no clicks or typing.
  case lookOnly
  /// Agents browse, click around and type drafts, and ask once before sending; the default.
  case ask
  /// Agents do everything without asking.
  case trusted

  /// The level's words, on its segment and in its menu.
  public var title: String {
    switch self {
    case .lookOnly: "Look Only"
    case .ask: "Ask Before Sending"
    case .trusted: "Trusted"
    }
  }
}
