/// The account form's screen. The fields themselves are the window's `OnboardingAccountForm`.
package struct OnboardingCreateScreenState: Equatable, Sendable {
  /// The headline's second line: what macOS asks next, or to confirm in its dialog while the account is made.
  package var tail: String
  /// What is wrong with the password, shown in place of the work note; nil when nothing is.
  package var passwordProblem: String?
  /// Whether the fields can be edited: not while the account is made.
  package var isFormEnabled: Bool

  /// Creates the account form's screen state.
  ///
  /// - Parameters:
  ///   - tail: The headline's second line: what macOS asks next, or to confirm in its dialog while the account is made.
  ///   - passwordProblem: What is wrong with the password, shown in place of the work note; nil when nothing is.
  ///   - isFormEnabled: Whether the fields can be edited: not while the account is made.
  package init(tail: String, passwordProblem: String? = nil, isFormEnabled: Bool) {
    self.tail = tail
    self.passwordProblem = passwordProblem
    self.isFormEnabled = isFormEnabled
  }
}
