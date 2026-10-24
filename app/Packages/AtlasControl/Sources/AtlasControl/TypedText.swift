/// Committed text as the helper takes it: the chunks to type, in order, or why it is refused. Text is counted in
/// Unicode characters (code points), as the helper counts it.
public enum TypedText: Equatable, Sendable {
  /// The most characters one paste may carry: 2048, sixteen full chunks, the most Atlas has ever typed at once.
  public static let pasteLimit = 2048
  /// The most characters the helper accepts in one `type` action: 256, its own limit (`src/lib/actions.rs`).
  static let chunkLimit = 256
  /// The most characters a chunk of whole graphemes holds: 128, half the helper's ``chunkLimit``, so a chunk stays
  /// well inside it, and a grapheme longer than this still goes whole in a chunk of its own.
  static let chunkSize = 128

  /// The text as chunks of whole graphemes, never splitting one: each holds at most ``chunkSize`` characters, except
  /// a chunk of one grapheme longer than that, which goes whole and may hold up to ``chunkLimit``.
  case chunks([String])
  /// The text is longer than ``pasteLimit``.
  case tooLong
  /// One grapheme of the text is longer than ``chunkLimit``, so it cannot be typed whole.
  case sequenceTooLong

  /// Splits `text` into the chunks the helper accepts, or refuses it.
  ///
  /// - Parameter text: The committed or pasted text; empty text is no chunks.
  public init(_ text: String) {
    guard text.unicodeScalars.count <= Self.pasteLimit else {
      self = .tooLong
      return
    }
    let chunks = text.typedChunks(size: Self.chunkSize)
    guard chunks.allSatisfy({ $0.unicodeScalars.count <= Self.chunkLimit }) else {
      self = .sequenceTooLong
      return
    }
    self = .chunks(chunks)
  }
}
