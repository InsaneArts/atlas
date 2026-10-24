import AtlasCommands

/// The setup window's words that depend on its state: what each command says while it runs, and every word a screen
/// shows that names an account, an agent, a permission or a state.
///
/// The rule every window follows: fixed copy stays in AtlasUI's views; a presenter supplies only the words that
/// depend on state, interpolated, chosen by a condition or derived from facts, and a list row's fixed name travels
/// with its value. Words the window shares with the panel are the app's shared ``Words``.
enum OnboardingWords {

  /// The first reading gave a reply that is not a status.
  static let notUnderstood = "Atlas gave an answer this window does not understand"
  /// While the account is made.
  static let creating =
    "Approve in the macOS dialog. It says Atlas can change your Mac. That covers only the three steps above."
  /// While the chosen agents are connected.
  static let connecting = "Connecting your agents…"
  /// While Open at Login is set.
  static let finishing = "Finishing…"
  /// While Atlas opens in the menu bar.
  static let opening = "Opening Atlas…"
  /// While the helper allows the account that asked.
  static let settingUpHere = "Setting Atlas up in this account…"

  /// While the helper asked is let go, to choose another.
  static let backToChoice = "Going back to the choice…"

  /// While an existing account is set up as the helper, which macOS asks to approve.
  ///
  /// - Parameter account: The account as people know it.
  /// - Returns: "Approve in the macOS dialog to get Test Account ready."
  static func settingUp(_ account: String) -> String {
    "Approve in the macOS dialog to get \(named(account)) ready."
  }

  /// While the account Atlas was set up in before is chosen again, with no approval.
  ///
  /// - Parameter account: The account as people know it.
  /// - Returns: "Setting up Test Account…"
  static func choosing(_ account: String) -> String {
    "Setting up \(named(account))…"
  }

  /// An existing account picked, which macOS will ask to approve getting ready.
  ///
  /// - Parameter account: The account as people know it.
  /// - Returns: "You'll approve once in macOS so Atlas can get Annie ready: …"
  static func approveToReady(_ account: String) -> String {
    "You'll approve once in macOS so Atlas can get \(named(account)) ready: Atlas opens there by itself, and an "
      + "account never used yet skips macOS's welcome questions. Nothing in the account is removed or replaced."
  }

  /// An account named in a sentence: the default helper's name, Atlas, is also the app's, so it is named as the Atlas
  /// account; any other name reads as itself.
  ///
  /// - Parameter account: The account as people know it.
  /// - Returns: "the Atlas account", or the name itself.
  static func named(_ account: String) -> String {
    account.caseInsensitiveCompare("Atlas") == .orderedSame ? "the \(account) account" : account
  }

  /// An account named at the start of a sentence.
  ///
  /// - Parameter account: The account as people know it.
  /// - Returns: "The Atlas account", or the name itself.
  static func namedFirst(_ account: String) -> String {
    let words = named(account)
    let first = words.prefix(1)
      .uppercased()
    return first + words.dropFirst()
  }

  /// An agent that could not be connected.
  ///
  /// - Parameters:
  ///   - agent: The agent.
  ///   - message: Why, in one line; empty when the command gave no reason.
  /// - Returns: "Codex: not installed."
  static func notConnected(_ agent: AgentID, _ message: String) -> String {
    let reason = message.isEmpty ? "it didn't connect" : message
    return "\(agentTitle(agent)): \(Words.sentence(reason))"
  }
}
