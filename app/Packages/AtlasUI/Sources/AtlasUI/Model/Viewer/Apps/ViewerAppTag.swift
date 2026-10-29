/// A short word on an app's card, such as "In front", "Hidden" or "From a copy".
public struct ViewerAppTag: Equatable, Sendable, Identifiable {
  /// The word.
  public var words: String
  /// How it is coloured.
  public var tone: Tone

  /// Creates a tag.
  ///
  /// - Parameters:
  ///   - words: The word.
  ///   - tone: How it is coloured.
  public init(words: String, tone: Tone) {
    self.words = words
    self.tone = tone
  }

  /// The tag's identity on its card: its word.
  public var id: String { words }
}
