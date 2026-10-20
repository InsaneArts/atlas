/// The line under a screen's controls: what runs now, what went wrong last, or nothing.
package enum WorkNote: Equatable, Sendable {
  /// Nothing runs and nothing went wrong.
  case none
  /// A command runs.
  ///
  /// - Parameter text: What it does, in words.
  case working(String)
  /// The last command failed.
  ///
  /// - Parameter text: What went wrong, in one line.
  case problem(String)
}
