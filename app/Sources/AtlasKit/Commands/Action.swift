/// Every `atlas` command the app runs, so each argument list is written, and tested, once.
package enum Action: Sendable, Equatable {
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
  /// Teach the project in this folder.
  case teachProject(path: String)

  /// Recover as the viewer service's own Recover does: restart the helper while an action's outcome is unknown or
  /// under way, else start its input, which never needs the full restart.
  ///
  /// - Parameter overview: The service's latest overview.
  /// - Returns: ``restart`` or ``startInput``.
  package static func recover(_ overview: ViewerStatus.Overview) -> Action {
    overview.recoveryRestarts ? .restart : .startInput
  }

  /// The `atlas` arguments the action runs, after `--json`.
  package var arguments: [String] {
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
    case .finishSetup: ["onboarding", "finish"]
    case .setOpensAtLogin(let on): ["onboarding", "login", on ? "on" : "off"]
    case .openMenuBar: ["menubar"]
    case .openAtLogin(let on): ["menubar", "--login", on ? "on" : "off"]
    case .allow(let uid, let name):
      ["onboarding", "accept", "--controller-uid", String(uid), "--controller-name", name]
    case .markWindow(let open): ["onboarding", "window", open ? "open" : "closed"]
    case .agentsStatus: ["agents", "status"]
    case .installAgents(let agents): ["agents", "install"] + agents.map { "--\($0.rawValue)" }
    case .teachProject(let path): ["agents", "project", path]
    }
  }
}
