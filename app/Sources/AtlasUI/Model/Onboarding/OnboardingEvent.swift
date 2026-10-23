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
  /// Continue on the agents screen: connect the agents.
  case connectAgents
  /// Open Atlas at login turned on or off.
  case setOpenAtLogin(Bool)
  /// Done.
  case complete
  /// Allow, in the helper.
  case allow
  /// Allow a permission: open its page in System Settings, with the permission panel beside it.
  ///
  /// - Parameter permission: The permission's id.
  case openSettings(id: String)
  /// Back on the permission panel: the permissions screen again.
  case closePanel
  /// Switch Back, in the helper.
  case switchBack
}
