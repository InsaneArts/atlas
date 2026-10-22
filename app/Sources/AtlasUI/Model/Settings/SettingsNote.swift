/// A line under a settings group saying what a command just did: quiet small print, or, for a problem, in the
/// attention colour.
package struct SettingsNote: Equatable, Sendable {
  /// The line.
  package var text: String
  /// Whether it says something went wrong or could not run.
  package var isProblem: Bool

  /// Creates a note.
  ///
  /// - Parameters:
  ///   - text: The line.
  ///   - isProblem: Whether it says something went wrong or could not run.
  package init(text: String, isProblem: Bool) {
    self.text = text
    self.isProblem = isProblem
  }
}
