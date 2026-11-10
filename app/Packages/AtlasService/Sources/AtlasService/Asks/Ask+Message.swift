extension Ask {
  /// The message a Return would send, as the person sees it before they answer.
  public struct Message: Decodable, Equatable, Sendable {
    /// Its start, as much as the service shows.
    public var preview: String
    /// How many characters it holds.
    public var length: Int

    /// Creates a message.
    ///
    /// - Parameters:
    ///   - preview: Its start.
    ///   - length: How many characters it holds.
    public init(preview: String, length: Int) {
      self.preview = preview
      self.length = length
    }

    /// The fields of a message.
    enum CodingKeys: String, CodingKey {
      /// Each field by the name the service writes.
      case preview, length
    }

    /// Decodes a message.
    ///
    /// - Parameter decoder: The decoder.
    /// - Throws: `DecodingError` when it has no preview.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      preview = try container.decode(String.self, forKey: .preview)
      length = container.lenientCount(.length)
    }
  }
}
