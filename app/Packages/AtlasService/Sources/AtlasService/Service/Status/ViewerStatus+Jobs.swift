extension ViewerStatus {
  /// The background jobs agents keep running in the helper, as the service counts them.
  public struct Jobs: Decodable, Equatable, Sendable {
    /// How many may still run: started and not yet seen to end.
    public var running = 0

    /// Creates a count of no jobs.
    public init() {}

    /// The fields of the `jobs` object.
    private enum CodingKeys: String, CodingKey {
      case running
    }

    /// Decodes the count, keeping 0 when it is missing, of the wrong type or below 0.
    ///
    /// - Parameter decoder: The decoder reading the `jobs` object.
    /// - Throws: `DecodingError` only when it is not an object.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      running = max(0, (try? container.decodeIfPresent(Int.self, forKey: .running)) ?? 0)
    }
  }
}
