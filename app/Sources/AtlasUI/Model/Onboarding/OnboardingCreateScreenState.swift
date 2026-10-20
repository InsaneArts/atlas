/// The account form's screen. The fields themselves are the window's `OnboardingAccountForm`.
package struct OnboardingCreateScreenState: Equatable, Sendable {
  /// The headline's second line: what macOS asks next, or to confirm in its dialog while the account is made.
  package var tail: String
  /// The line under the form: what is wrong with the password, else the window's work note.
  package var note: WorkNote
  /// Whether the fields can be edited: not while the account is made.
  package var isFormEnabled: Bool

  /// Creates the account form's screen state.
  ///
  /// - Parameters:
  ///   - tail: The headline's second line: what macOS asks next, or to confirm in its dialog while the account is made.
  ///   - note: The line under the form: what is wrong with the password, else the window's work note.
  ///   - isFormEnabled: Whether the fields can be edited: not while the account is made.
  package init(tail: String, note: WorkNote = .none, isFormEnabled: Bool) {
    self.tail = tail
    self.note = note
    self.isFormEnabled = isFormEnabled
  }
}
