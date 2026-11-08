extension ActionFacts {
  /// How far a scroll went, in lines: down and right are above 0.
  public struct Scroll: Decodable, Equatable, Sendable {
    /// Across.
    public var dx: Double
    /// Down.
    public var dy: Double

    /// Creates a scroll.
    ///
    /// - Parameters:
    ///   - dx: Across.
    ///   - dy: Down.
    public init(dx: Double, dy: Double) {
      self.dx = dx
      self.dy = dy
    }

    /// The fields of a scroll.
    enum CodingKeys: String, CodingKey {
      /// Each by the name the service writes.
      case dx, dy
    }

    /// Decodes a scroll, a missing or odd direction reading as none.
    ///
    /// - Parameter decoder: The decoder.
    /// - Throws: `DecodingError` only when the scroll is not an object.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      dx = (try? container.finite(.dx)) ?? 0
      dy = (try? container.finite(.dy)) ?? 0
    }
  }
}
