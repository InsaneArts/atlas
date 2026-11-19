/// The onboarding's screens, in the order the person meets them. The person's screens come first, the VM's beside
/// Second Login's, then the helper's, then `closeLeftovers` and `blocked`.
public enum OnboardingStep: Equatable, Hashable, Sendable {
  /// What Atlas is, before anything is set up.
  case welcome
  /// Choose where agents work: Second Login, a VM or, later, Cloud.
  case chooseDesktop
  /// macOS comes down from Apple for the VM.
  case downloadVM
  /// Atlas makes the VM.
  case installVM
  /// Allow Atlas in the VM's own window.
  case allowVM
  /// Choose an account to be the helper.
  case chooseHelper
  /// Create a new account to be the helper.
  case createAccount
  /// Log in to the helper once and allow Atlas there.
  case switchToHelper
  /// Atlas checks the helper works.
  case finishing
  /// Connect agents.
  case agents
  /// Setup is done.
  case done

  /// Allow the account that asked.
  case allow
  /// Grant Accessibility and Screen Recording.
  case permissions
  /// The helper is set up; switch back.
  case helperDone

  /// A deleted account's login is still open: Atlas closes it before anything goes on, then says so.
  case closeLeftovers

  /// What only the person can fix, in words.
  case blocked(String)
}
