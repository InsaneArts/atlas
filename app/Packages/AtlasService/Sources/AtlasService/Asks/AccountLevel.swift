/// How freely agents act in an account the person listed: Look Only lets them read it, Ask Before Sending (the default)
/// asks the person once before something leaves the Mac, and Trusted lets them do everything.
public enum AccountLevel: String, Codable, Equatable, Sendable, CaseIterable {
  /// Agents read, scroll and take screenshots; no clicks or typing.
  case lookOnly
  /// Agents browse, click around and type drafts, and ask once before sending.
  case ask
  /// Agents do everything without asking.
  case trusted
}
