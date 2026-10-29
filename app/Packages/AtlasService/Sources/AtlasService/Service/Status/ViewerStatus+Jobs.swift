extension ViewerStatus {
  /// The background jobs agents keep running in the helper, as the service counts them.
  public struct Jobs: Decodable, Equatable, Sendable {
    /// How many may still run: started and not yet seen to end.
    public var running = 0
    /// How many are being started now.
    public var launching = 0
    /// How many the helper keeps records of it cannot read: they may still run.
    public var unresolved = 0

    /// Whether any job may still run, as far as the helper can tell: running, starting, or of a record it cannot
    /// read.
    public var mayRun: Bool { running + launching + unresolved > 0 }

    /// Creates a count of no jobs.
    public init() {}

    /// The fields of the `jobs` object.
    private enum CodingKeys: String, CodingKey {
      case running, launching, unresolved
    }

    /// Decodes the counts, keeping 0 for each that is missing, of the wrong type or below 0.
    ///
    /// - Parameter decoder: The decoder reading the `jobs` object.
    /// - Throws: `DecodingError` only when it is not an object.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      running = max(0, (try? container.decodeIfPresent(Int.self, forKey: .running)) ?? 0)
      launching = max(0, (try? container.decodeIfPresent(Int.self, forKey: .launching)) ?? 0)
      unresolved = max(0, (try? container.decodeIfPresent(Int.self, forKey: .unresolved)) ?? 0)
    }
  }
}
