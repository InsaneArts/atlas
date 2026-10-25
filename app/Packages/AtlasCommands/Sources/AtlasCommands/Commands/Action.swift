import AtlasService

/// Every `atlas` command the app runs, so each argument list is written, and tested, once.
public enum Action: Sendable, Equatable {
  /// Lock the screen to switch to this account (the helper, or back).
  case switchTo(String)
  /// Lock this account's screen, with no account named to switch to.
  case lockScreen
  /// List this Mac's accounts and which one is the helper.
  case listAccounts
  /// Restart the helper, settling an action of unknown outcome.
  case restart
  /// Start, reset or update the helper's input.
  case startInput
  /// Log the helper out; with `force`, even past an app that holds it up.
  case logOut(force: Bool)
  /// Read what the setup window needs to choose its step.
  case onboardingStatus
  /// Set an existing account up as the helper.
  case chooseHelper(String)
  /// Create a new helper account; its password travels on standard input.
  case createAccount(account: String, fullName: String)
  /// Take back the request of a helper that never answered, to choose another account.
  case forgetHelper
  /// Close the logins deleted accounts left open; macOS asks the person to approve.
  case closeLeftovers
  /// Log out the helper whose Atlas isn't answering, as root; macOS asks the person to approve.
  case closeHelper
  /// Run the end-to-end check of the helper.
  case finishSetup
  /// Turn Open at Login on or off at the end of setup.
  case setOpensAtLogin(Bool)
  /// Turn Open at Login on or off from the menu bar, recorded as the person's choice.
  case openAtLogin(Bool)
  /// Open Atlas in the menu bar.
  case openMenuBar
  /// In the helper: allow the account that asked, exactly as shown.
  case allow(uid: Int, name: String)
  /// In the helper: tell Atlas whether the setup window is open, so its own notice stays quiet meanwhile.
  case markWindow(open: Bool)
  /// Read which agents are installed and connected.
  case agentsStatus
  /// Connect these agents.
  case installAgents([AgentID])
  /// Connect or disconnect the named CLI components in one command.
  case changeAgentParts([String], remove: Bool)
  /// Connect the Claude app alone.
  case connectClaudeApp
  /// Take the Atlas section out of the project in this folder, and off the list of projects taught.
  case forgetProject(path: String)
  /// Diagnose the helper, with its full status.
  case doctor
  /// Ask whether a newer Atlas is published, changing nothing.
  case checkForUpdate
  /// Install the newer Atlas, checking its signature first.
  case update

  /// Recover as the viewer service's own Recover does: restart the helper while an action's outcome is unknown or
  /// under way, else start its input, which never needs the full restart.
  ///
  /// - Parameter overview: The service's latest overview.
  /// - Returns: ``restart`` or ``startInput``.
  public static func recover(_ overview: ViewerStatus.Overview) -> Action {
    overview.recoveryRestarts ? .restart : .startInput
  }

  /// The `atlas` arguments the action runs, after `--json`.
  public var arguments: [String] {
    switch self {
    // The person clicked Switch: that is the confirmation.
    case .switchTo(let account): ["switch", account, "--yes"]
    case .lockScreen: ["switch", "--lock", "--yes"]
    case .listAccounts: ["switch", "--list"]
    case .restart: ["restart"]
    case .startInput: ["start-input"]
    // The person confirmed in the panel.
    case .logOut(let force): ["logout", "--yes"] + (force ? ["--force"] : [])
    case .onboardingStatus: ["onboarding", "status"]
    case .chooseHelper(let account): ["onboarding", "choose", account]
    case .createAccount(let account, let fullName):
      ["onboarding", "create", "--account", account, "--full-name", fullName]
    case .forgetHelper: ["onboarding", "forget"]
    // macOS's approval dialog is the confirmation.
    case .closeLeftovers: ["onboarding", "close"]
    case .closeHelper: ["onboarding", "close", "--helper"]
    case .finishSetup: ["onboarding", "finish"]
    case .setOpensAtLogin(let on): ["onboarding", "login", on ? "on" : "off"]
    case .openMenuBar: ["menubar"]
    case .openAtLogin(let on): ["menubar", "--login", on ? "on" : "off"]
    case .allow(let uid, let name):
      ["onboarding", "accept", "--controller-uid", String(uid), "--controller-name", name]
    case .markWindow(let open): ["onboarding", "window", open ? "open" : "closed"]
    case .agentsStatus: ["agents", "status"]
    case .installAgents(let agents): ["agents", "install"] + agents.map { "--\($0.rawValue)" }
    case .changeAgentParts(let parts, let remove):
      ["agents", remove ? "remove" : "install"]
        + parts.map { $0 == AgentsStatus.claudeAppID ? "--app" : "--\($0)" }
    case .connectClaudeApp: ["agents", "install", "--app"]
    // The person confirmed in Settings.
    case .forgetProject(let path): ["agents", "forget", path]
    case .doctor: ["doctor", "--verbose"]
    // Only asks: the check downloads nothing and installs nothing.
    case .checkForUpdate: ["update", "--check"]
    // The person confirmed in Settings.
    case .update: ["update"]
    }
  }
}
