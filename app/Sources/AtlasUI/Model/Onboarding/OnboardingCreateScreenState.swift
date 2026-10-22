/// The account form's screen. The fields themselves are the window's `OnboardingAccountForm`.
package struct OnboardingCreateScreenState: Equatable, Sendable {
  /// The headline's second line: what macOS asks next, or to confirm in its dialog while the account is made.
  package var tail: String
  /// The line under the form: what is wrong with the password, else the window's work note.
  package var note: WorkNote
  /// Whether the fields can be edited: not while the account is made.
  package var isFormEnabled: Bool
  /// The field the note's problem is about, which the form marks; nil marks none.
  package var flagged: OnboardingAccountField?

  /// Creates the account form's screen state.
  ///
  /// - Parameters:
  ///   - tail: The headline's second line: what macOS asks next, or to confirm in its dialog while the account is made.
  ///   - note: The line under the form: what is wrong with the password, else the window's work note.
  ///   - isFormEnabled: Whether the fields can be edited: not while the account is made.
  ///   - flagged: The field the note's problem is about, which the form marks; nil marks none.
  package init(tail: String, note: WorkNote = .none, isFormEnabled: Bool, flagged: OnboardingAccountField? = nil) {
    self.tail = tail
    self.note = note
    self.isFormEnabled = isFormEnabled
    self.flagged = flagged
  }
}
