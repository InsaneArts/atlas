extension ViewerStatus {
  /// A display's size, in points.
  public struct Size: Decodable, Equatable, Sendable {
    /// The width, in points.
    public var width: Double
    /// The height, in points.
    public var height: Double

    /// Whether both sides are finite, positive and within the supported display bounds.
    public var isValid: Bool {
      width.isFinite && height.isFinite && width > 0 && height > 0
        && width <= Self.maximumSide && height <= Self.maximumSide
    }

    /// The largest supported display side, in points.
    private static let maximumSide: Double = 32768

    /// Creates a size.
    ///
    /// - Parameters:
    ///   - width: The width, in points.
    ///   - height: The height, in points.
    public init(width: Double, height: Double) {
      self.width = width
      self.height = height
    }

    /// The dimensions of a display.
    private enum CodingKeys: String, CodingKey {
      case width, height
    }

    /// Decodes only supported display geometry.
    ///
    /// - Parameter decoder: The decoder reading the size.
    /// - Throws: `DecodingError` when either side is missing or outside the supported bounds.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      width = try container.decode(Double.self, forKey: .width)
      height = try container.decode(Double.self, forKey: .height)
      guard isValid else {
        throw DecodingError.dataCorrupted(
          .init(codingPath: decoder.codingPath, debugDescription: "Invalid display size")
        )
      }
    }
  }
}
