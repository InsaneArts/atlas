import AtlasKit

/// The setup window's words that depend on its state: what each command says while it runs, and every word a screen
/// shows that names an account, an agent, a permission or a state.
///
/// The rule every window follows: fixed copy stays in AtlasUI's views; a presenter supplies only the words that
/// depend on state, interpolated, chosen by a condition or derived from facts, and a list row's fixed name travels
/// with its value. Words the window shares with the panel are AtlasKit's ``Words``.
enum OnboardingWords {
  // MARK: Commands

  /// The first reading gave a reply that is not a status.
  static let notUnderstood = "Atlas gave an answer this window does not understand"
  /// While the account is made.
  static let creating = "Creating the account…"
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

  /// While an existing account is set up as the helper.
  ///
  /// - Parameter account: The account as people know it.
  /// - Returns: "Setting up Test Account…"
  static func settingUp(_ account: String) -> String {
    "Setting up \(named(account))…"
  }

  // MARK: Naming an account

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

  /// While a project is taught.
  ///
  /// - Parameter project: The folder's name.
  /// - Returns: "Teaching Opie…"
  static func teaching(_ project: String) -> String {
    "Teaching \(project)…"
  }

  /// A project that could not be taught.
  ///
  /// - Parameters:
  ///   - project: The folder's name.
  ///   - message: Why, in one line.
  /// - Returns: "Opie: why."
  static func notTaught(_ project: String, _ message: String) -> String {
    "\(project): \(message)."
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

  // MARK: The account choice and form

  /// The form's tail while the account is made.
  static let confirmInDialog = "Confirm in the macOS dialog."
  /// The form's tail before.
  static let passwordNext = "macOS asks for your password next."

  /// A standard account's row detail.
  ///
  /// - Parameter account: Its short name.
  /// - Returns: "testaccount · standard account on this Mac"
  static func candidateDetail(_ account: String) -> String {
    "\(account) · standard account on this Mac"
  }

  // MARK: The switch

  /// The switch screen's second step, before the helper has logged in.
  static let answerWelcome = "Answer the welcome questions, then open Atlas there"
  /// Atlas answers in the helper, which still lacks a permission.
  static let waitingForPermissions = "Waiting for the two permissions…"
  /// The helper is ready, on screen.
  static let waitingForSwitchBack = "Waiting for you to switch back…"
  /// Atlas runs in the helper, but the helper is not on screen.
  static let lockThenOpenAtlas = "Next: Lock and Switch, then open Atlas there."

  /// The switch screen's headline.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "Now log in to the Atlas account."
  static func logIn(to helper: String) -> String {
    "Now log in to \(named(helper))."
  }

  /// Where to choose the helper.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "Choose the Atlas account on the lock screen"
  static func chooseOnLockScreen(_ helper: String) -> String {
    "Choose \(named(helper)) on the lock screen"
  }

  /// The switch screen's second step, once the helper has logged in.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "Open Atlas in the Atlas account"
  static func openAtlas(in helper: String) -> String {
    "Open Atlas in \(named(helper))"
  }

  /// The helper has not logged in yet.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "Next: Lock and Switch, then log in to the Atlas account."
  static func lockThenLogIn(to helper: String) -> String {
    "Next: Lock and Switch, then log in to \(named(helper))."
  }

  /// The helper is on screen, and Atlas not open there yet.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "Waiting for Atlas to open in the Atlas account…"
  static func waitingForAtlas(in helper: String) -> String {
    "Waiting for Atlas to open in \(named(helper))…"
  }

  // MARK: Finishing

  /// The headline while the check runs or failed.
  static let checking = "Checking everything"
  /// Its tail.
  static let endToEnd = "end to end."
  /// The headline once the check passed.
  static let everythingWorks = "Everything works."
  /// Its tail.
  static let agentsHaveAMac = "Your agents have a Mac now."
  /// The headline once the check failed.
  static let almostThere = "Almost there."
  /// Its tail.
  static let oneCheckFailed = "One check didn't pass."
  /// The live check's note while it runs.
  static let checkingNote = "checking…"
  /// Its note once passed.
  static let passed = "passed"
  /// Its note once failed.
  static let failed = "failed"

  /// The first check, naming the helper.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "The Atlas account is logged in"
  static func loggedIn(_ helper: String) -> String {
    "\(namedFirst(helper)) is logged in"
  }

  // MARK: Agents and done

  /// An agent's name.
  ///
  /// - Parameter agent: The agent.
  /// - Returns: "Claude Code" or "Codex".
  static func agentTitle(_ agent: AgentID) -> String {
    Words.agentTitle(agent)
  }

  /// An agent that already knows Atlas.
  static let agentConnected = "Connected. Every session has the Atlas skill."
  /// An agent installed and not yet connected.
  static let agentFound = "Found. Adds the Atlas skill to every session."
  /// An agent not on this Mac.
  static let agentMissing = "Not installed on this Mac."
  /// An agent while `atlas agents status` runs.
  static let agentChecking = "Checking…"
  /// An agent `atlas agents status` could not check.
  static let agentNotChecked = "Not checked yet."
  /// `atlas agents status` failed: the step can be skipped, and Atlas asks again.
  static let agentsCheckFailed =
    "Atlas couldn't check your agents. Continue to skip this, or wait while it tries again."

  /// A chosen project's remove button, for VoiceOver.
  ///
  /// - Parameter project: The folder's name.
  /// - Returns: "Remove Opie"
  static func remove(_ project: String) -> String {
    "Remove \(project)"
  }

  /// The done screen's lede.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: What Atlas does from now on, and what to do after a restart.
  static func doneLede(_ helper: String) -> String {
    "That was the whole setup. Atlas lives in your menu bar: ask your agent to check something on screen, and watch it work from there. After a restart, just log in to \(named(helper)) again; Atlas does the rest."
  }

  // MARK: The helper's side

  /// A permission that is on.
  static let permissionOn = "Allowed"
  /// A permission that is off.
  static let permissionOff = "Off"
  /// The helper's done tail when no account asked.
  static let switchBackToOwn = "Switch back to your own account."

  /// The allow screen's label over its title.
  ///
  /// - Parameter account: This account as people know it.
  /// - Returns: "Atlas in the Atlas account"
  static func atlasIn(_ account: String) -> String {
    "Atlas in \(named(account))"
  }

  /// The allow screen's headline.
  ///
  /// - Parameter asker: The asking account as people know it.
  /// - Returns: "Let Ismayil use this account"
  static func letUse(_ asker: String) -> String {
    "Let \(named(asker)) use this account"
  }

  /// What allowing means.
  ///
  /// - Parameter asker: The asking account as people know it.
  /// - Returns: The allow screen's lede.
  static func allowLede(_ asker: String) -> String {
    "\(asker)'s agents will open apps here, click and type, while \(asker) keeps working in their own account. You allow this once; Atlas then starts here by itself."
  }

  /// The button that opens a permission's switch.
  ///
  /// - Parameter permission: The permission's name.
  /// - Returns: "Allow Accessibility"
  static func allow(_ permission: String) -> String {
    "Allow \(permission)"
  }

  /// The guide's label over its title.
  ///
  /// - Parameters:
  ///   - number: Which permission, from 1.
  ///   - count: How many.
  ///   - permission: Its name.
  /// - Returns: "1 of 2 · Accessibility"
  static func guideLabel(_ number: Int, of count: Int, _ permission: String) -> String {
    "\(number) of \(count) · \(permission)"
  }

  /// Where the switch is.
  ///
  /// - Parameter permission: The permission's name.
  /// - Returns: "Privacy & Security › Accessibility"
  static func settingsPath(_ permission: String) -> String {
    "Privacy & Security › \(permission)"
  }

  /// The guide's pill once the switch is on.
  ///
  /// - Parameter permission: The permission's name.
  /// - Returns: "Accessibility is on"
  static func isOn(_ permission: String) -> String {
    "\(permission) is on"
  }

  /// The guide's pill while it waits.
  ///
  /// - Parameter permission: The permission's name.
  /// - Returns: "Waiting for Accessibility…"
  static func waiting(for permission: String) -> String {
    "Waiting for \(permission)…"
  }

  /// The helper's done tail.
  ///
  /// - Parameter asker: The account to switch back to, as people know it.
  /// - Returns: "Switch back to Ismayil."
  static func switchBack(to asker: String) -> String {
    "Switch back to \(named(asker))."
  }
}
