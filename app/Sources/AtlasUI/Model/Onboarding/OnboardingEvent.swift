/// What the person did in the setup window.
package enum OnboardingEvent: Equatable, Sendable {
  /// Get Started on the welcome.
  case begin
  /// Back, to the screen before.
  case back
  /// An account picked in the choice.
  ///
  /// - Parameter account: A new account, or the short name of one already on this Mac.
  case pick(OnboardingAccountPick)
  /// Continue on the account choice.
  case continueChoice
  /// Create Account on the form.
  case createAccount
  /// Lock and Switch, to log in to the helper.
  case lockAndSwitch
  /// Choose Another Account, while the helper asked never answered.
  case chooseAnother
  /// Check Again when blocked: read the facts again.
  case checkAgain
  /// Try Again after a failed check.
  case retryFinish
  /// Continue once the check passed: on to what the facts lead to next.
  case continueFinishing
  /// An agent's switch turned on or off.
  ///
  /// - Parameters:
  ///   - id: The agent's id.
  ///   - on: Whether it is on.
  case setAgent(id: String, on: Bool)
  /// Choose Folder…, to teach a project.
  case chooseProject
  /// A chosen project removed.
  ///
  /// - Parameter id: The project's id.
  case removeProject(id: String)
  /// Continue on the agents screen: connect the agents and teach the projects.
  case connectAgents
  /// Open Atlas at login turned on or off.
  case setOpenAtLogin(Bool)
  /// Done.
  case complete
  /// Allow, in the helper.
  case allow
  /// Allow a permission: open its page in System Settings, with the guide beside it.
  ///
  /// - Parameter permission: The permission's id.
  case openSettings(id: String)
  /// Back on the guide panel.
  case closeGuide
  /// Switch Back, in the helper.
  case switchBack
}
