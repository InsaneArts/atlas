extension ActionFacts {
  /// A point on the helper's display, in points from its top-left corner.
  public struct Point: Decodable, Equatable, Sendable {
    /// Across, from the left edge.
    public var x: Double
    /// Down, from the top edge.
    public var y: Double

    /// Creates a point.
    ///
    /// - Parameters:
    ///   - x: Across, from the left edge.
    ///   - y: Down, from the top edge.
    public init(x: Double, y: Double) {
      self.x = x
      self.y = y
    }

    /// The fields of a point.
    enum CodingKeys: String, CodingKey {
      /// Each by the name the service writes.
      case x, y
    }

    /// Decodes a point.
    ///
    /// - Parameter decoder: The decoder.
    /// - Throws: `DecodingError` when either value is missing, of the wrong type or not finite.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      x = try container.finite(.x)
      y = try container.finite(.y)
    }
  }
}
