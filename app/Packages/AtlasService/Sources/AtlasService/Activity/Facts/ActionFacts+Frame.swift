extension ActionFacts {
  /// Where an element stands on the helper's display, in points from its top-left corner.
  public struct Frame: Decodable, Equatable, Sendable {
    /// Its left edge.
    public var x: Double
    /// Its top edge.
    public var y: Double
    /// Its width.
    public var width: Double
    /// Its height.
    public var height: Double

    /// Creates a frame.
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

    /// The fields of a frame.
    enum CodingKeys: String, CodingKey {
      /// Each by the name the service writes.
      case x, y, width, height
    }

    /// Decodes a frame.
    ///
    /// - Parameter decoder: The decoder.
    /// - Throws: `DecodingError` when a value is missing, of the wrong type or not finite, or the frame is empty.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      x = try container.finite(.x)
      y = try container.finite(.y)
      width = try container.finite(.width)
      height = try container.finite(.height)
      guard width > 0, height > 0 else {
        throw DecodingError.dataCorruptedError(forKey: .width, in: container, debugDescription: "An empty frame")
      }
    }
  }
}
