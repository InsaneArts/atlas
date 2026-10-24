import AtlasCommands
import Foundation

/// The setup window's words that depend on its state: what each command says while it runs, and every word a screen
/// shows that names an account, an agent, a permission or a state.
///
/// The rule every window follows: fixed copy stays in AtlasUI's views; a presenter supplies only the words that
/// depend on state, interpolated, chosen by a condition or derived from facts, and a list row's fixed name travels
/// with its value. Words the window shares with the panel are the app's shared ``Words``; the words that say where a
/// permission or a check stands are ``StatusWord``'s.
enum OnboardingWords {

  /// The first reading gave a reply that is not a status.
  static let notUnderstood = "Atlas gave an answer this window does not understand"
  /// The first reading could not run Atlas's command line at all: a plain cause and what to do.
  static let filesIncomplete = "Atlas’s files look incomplete. Reinstall Atlas, then open it again."
  /// While the account is made.
  static let creating = "Approve in the macOS dialog. It says Atlas can change your Mac; that means only what’s below."
  /// While the chosen agents are connected.
  static let connecting = "Connecting your agents…"
  /// While Atlas reads which agents are on this Mac, which Continue waits for.
  static let checkingAgents = "Checking your agents…"
  /// While Open at Login is set.
  static let finishing = "Finishing…"
  /// While Atlas opens in the menu bar.
  static let opening = "Opening Atlas…"
  /// While the helper allows the account that asked.
  static let settingUpHere = "Setting Atlas up in this account…"

  /// While the helper asked is let go, to choose another.
  static let backToChoice = "Going back to the choice…"

  /// An existing account picked, which macOS will ask to approve getting ready. It names no account, as the row
  /// picked above it does.
  static let approveToReady =
    "You’ll approve once in macOS. "
    + "Atlas then opens there by itself and skips macOS’s welcome questions; nothing in the account is removed."

  /// Where the screen sits on the footer's track, as VoiceOver says it.
  ///
  /// - Parameters:
  ///   - index: The screen, from 0.
  ///   - count: How many screens its side of setup has.
  /// - Returns: "Step 2 of 6"
  static func step(_ index: Int, of count: Int) -> String {
    "Step \(index + 1) of \(count)"
  }

  /// While an existing account is set up as the helper, which macOS asks to approve.
  ///
  /// - Parameter account: The account as people know it.
  /// - Returns: "Approve in the macOS dialog to get Annie ready."
  static func settingUp(_ account: String) -> String {
    "Approve in the macOS dialog to get \(named(account)) ready."
  }

  /// While the account Atlas was set up in before is chosen again, with no approval.
  ///
  /// - Parameter account: The account as people know it.
  /// - Returns: "Setting up Annie…"
  static func choosing(_ account: String) -> String {
    "Setting up \(named(account))…"
  }

  /// An account named in a sentence: the default helper's name, Atlas, is also the app's, so it is named as the Atlas
  /// account; a person's full name is called by its given name, so a long name never floods the sentence; any other
  /// name reads as itself.
  ///
  /// - Parameter account: The account as people know it.
  /// - Returns: "the Atlas account", "Maria Fernanda" for "Maria Fernanda Montgomery-Castellanos de la Vega", or the
  ///   name itself.
  static func named(_ account: String) -> String {
    if account.caseInsensitiveCompare("Atlas") == .orderedSame { return "the \(account) account" }
    return givenName(account)
  }

  /// An account named at the start of a sentence.
  ///
  /// - Parameter account: The account as people know it.
  /// - Returns: "The Atlas account", or the name as ``named(_:)`` gives it.
  static func namedFirst(_ account: String) -> String {
    let words = named(account)
    let first = words.prefix(1)
      .uppercased()
    return first + words.dropFirst()
  }

  /// A full name's given name, where macOS's name reading finds one that starts the name and a family name after it,
  /// as in "Christopher Alexander Montgomery-Smith"; else the whole name, as for "Test Account" or "Ismayil", whose
  /// reading would pick a word that is no given name.
  ///
  /// - Parameter name: The full name.
  /// - Returns: The given name, or the whole name.
  static func givenName(_ name: String) -> String {
    let parts = PersonNameComponentsFormatter()
      .personNameComponents(from: name)
    guard let given = parts?.givenName, !given.isEmpty, parts?.familyName != nil, name.hasPrefix(given) else {
      return name
    }
    return given
  }

  /// An agent that could not be connected.
  ///
  /// - Parameters:
  ///   - agent: The agent.
  ///   - message: Why, in one line; empty when the command gave no reason.
  /// - Returns: "Codex: not installed."
  static func notConnected(_ agent: AgentID, _ message: String) -> String {
    let reason = message.isEmpty ? "it didn’t connect" : message
    return "\(agentTitle(agent)): \(Words.sentence(reason))"
  }
}
