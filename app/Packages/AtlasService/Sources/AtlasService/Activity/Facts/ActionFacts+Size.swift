extension ActionFacts {
  /// The size of the display a picture shows, in points.
  public struct Size: Decodable, Equatable, Sendable {
    /// Its width.
    public var width: Double
    /// Its height.
    public var height: Double

    /// Creates a size.
    ///
    /// - Parameters:
    ///   - width: Its width.
    ///   - height: Its height.
    public init(width: Double, height: Double) {
      self.width = width
      self.height = height
    }

    /// The fields of a size.
    enum CodingKeys: String, CodingKey {
      /// Each by the name the service writes.
      case width, height
    }

    /// Decodes a size.
    ///
    /// - Parameter decoder: The decoder.
    /// - Throws: `DecodingError` when either value is missing, of the wrong type, not finite or not above 0.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      width = try container.finite(.width)
      height = try container.finite(.height)
      guard width > 0, height > 0 else {
        throw DecodingError.dataCorruptedError(forKey: .width, in: container, debugDescription: "An empty display")
      }
    }
  }
}
