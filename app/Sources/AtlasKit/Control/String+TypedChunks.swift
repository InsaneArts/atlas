extension String {
  /// Committed text as type actions of at most `size` code points each,
  /// never splitting a grapheme (an emoji sequence, a letter and its marks).
  /// A single grapheme longer than `size` is its own chunk; callers refuse
  /// chunks the helper would not accept.
  ///
  /// - Parameter size: The most code points in one chunk; ``TypedText`` passes its ``TypedText/chunkSize``.
  /// - Returns: The chunks, in order; none for empty text.
  package func typedChunks(size: Int) -> [String] {
    var chunks: [String] = []
    var current = ""
    var length = 0
    for grapheme in self {
      let points = grapheme.unicodeScalars.count
      if !current.isEmpty && length + points > size {
        chunks.append(current)
        current = ""
        length = 0
      }
      current.append(grapheme)
      length += points
    }
    if !current.isEmpty { chunks.append(current) }
    return chunks
  }
}
