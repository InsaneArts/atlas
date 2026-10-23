/// One setup screen and what it shows, in the order the person meets them.
package enum OnboardingScreen: Equatable, Sendable {
  /// What Atlas is, before anything is set up.
  case welcome
  /// Choose an account to be the helper.
  case choose(OnboardingChooseScreenState)
  /// Create a new account to be the helper.
  case create(OnboardingCreateScreenState)
  /// Log in to the helper once and allow Atlas there.
  case switchToHelper(OnboardingSwitchScreenState)
  /// Atlas checks the helper works.
  case finishing(OnboardingFinishingScreenState)
  /// Connect agents.
  case agents(OnboardingAgentsScreenState)
  /// Setup is done.
  case done(OnboardingDoneScreenState)
  /// In the helper: allow the account that asked.
  case allow(OnboardingAllowScreenState)
  /// In the helper: grant Accessibility and Screen Recording.
  case permissions(OnboardingPermissionsScreenState)
  /// In the helper: set up; switch back.
  case helperDone(OnboardingHelperDoneScreenState)
  /// Something only the person can fix first.
  ///
  /// - Parameter reason: What to fix, in words.
  case blocked(String)
}
