/// A line under a settings group saying what a command just did: quiet small print, a spinner while something it
/// waits for runs, or, for a problem, an orange mark and words (v3 `.sgfoot`).
public struct SettingsNote: Equatable, Sendable {
  /// How the line reads.
  public enum Kind: Equatable, Sendable {
    /// Quiet small print.
    case quiet
    /// Something it waits for runs: a spinner beside the words.
    case working
    /// Something went wrong or could not run: an orange mark and words.
    case problem
  }

  /// The line.
  public var text: String
  /// How the line reads.
  public var kind: Kind

  /// Creates a note.
  ///
  /// - Parameters:
  ///   - text: The line.
  ///   - kind: How the line reads.
  public init(text: String, kind: Kind) {
    self.text = text
    self.kind = kind
  }

  /// Whether it says something went wrong or could not run.
  public var isProblem: Bool { kind == .problem }
}
