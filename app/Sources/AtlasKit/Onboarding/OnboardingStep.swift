/// The onboarding's screens, in the order the person meets them.
package enum OnboardingStep: Equatable, Hashable, Sendable {
  // MARK: In the person's account

  /// What Atlas is, before anything is set up.
  case welcome
  /// Choose an account to be the helper.
  case chooseHelper
  /// Create a new account to be the helper.
  case createAccount
  /// Log in to the helper once and allow Atlas there.
  case switchToHelper
  /// Atlas checks the helper works.
  case finishing
  /// Connect agents and teach projects.
  case agents
  /// Setup is done.
  case done

  // MARK: In the helper's account

  /// Allow the account that asked.
  case allow
  /// Grant Accessibility and Screen Recording.
  case permissions
  /// The helper is set up; switch back.
  case helperDone

  // MARK: Something only the person can fix first

  /// What only the person can fix, in words.
  case blocked(String)
}
