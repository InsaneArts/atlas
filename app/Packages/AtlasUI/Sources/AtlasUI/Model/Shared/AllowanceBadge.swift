/// What let an agent's action act in one of the person's listed accounts, on a small capsule beside it: "You allowed
/// for this task" or "You allowed once" in the accent, "Trusted" quiet.
public struct AllowanceBadge: Equatable, Sendable {
  /// The words, such as "You allowed once".
  public var words: String
  /// How it is coloured.
  public var tone: Tone

  /// Creates a badge.
  ///
  /// - Parameters:
  ///   - words: The words.
  ///   - tone: How it is coloured.
  public init(words: String, tone: Tone) {
    self.words = words
    self.tone = tone
  }
}
