extension Ask {
  /// Where the element stands in the ask's picture, in its pixels from the top-left corner.
  public struct Area: Decodable, Equatable, Sendable {
    /// Its left edge.
    public var x: Double
    /// Its top edge.
    public var y: Double
    /// Its width.
    public var width: Double
    /// Its height.
    public var height: Double

    /// Creates an area.
    ///
    /// - Parameters:
    ///   - x: Its left edge.
    ///   - y: Its top edge.
    ///   - width: Its width.
    ///   - height: Its height.
    public init(x: Double, y: Double, width: Double, height: Double) {
      self.x = x
      self.y = y
      self.width = width
      self.height = height
    }

    /// The fields of an area.
    enum CodingKeys: String, CodingKey {
      /// Each by the name the service writes.
      case x, y, width, height
    }

    /// Decodes an area.
    ///
    /// - Parameter decoder: The decoder.
    /// - Throws: `DecodingError` when a value is missing or not finite, or the area is empty.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      x = try container.finite(.x)
      y = try container.finite(.y)
      width = try container.finite(.width)
      height = try container.finite(.height)
      guard width > 0, height > 0 else {
        throw DecodingError.dataCorruptedError(forKey: .width, in: container, debugDescription: "An empty area")
      }
    }
  }
}
