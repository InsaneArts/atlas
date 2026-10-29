extension StopOutcome {
  /// One background job a Stop could not end, as the service names it.
  public struct Job: Decodable, Equatable, Sendable {
    /// The job's ID; empty when the service gave none.
    public var id = ""
    /// Why nothing was sent to it, such as `identity`; nil when the service gave no reason.
    public var reason: String?

    /// Creates a job.
    ///
    /// - Parameters:
    ///   - id: The job's ID.
    ///   - reason: Why nothing was sent to it.
    public init(id: String, reason: String? = nil) {
      self.id = id
      self.reason = reason
    }

    /// The fields of one job.
    private enum CodingKeys: String, CodingKey {
      case id, reason
    }

    /// Decodes a job, keeping it, with an empty ID, even when its fields are missing or of the wrong type: a job
    /// the Stop could not end is never dropped from the count.
    ///
    /// - Parameter decoder: The decoder reading one job.
    /// - Throws: `DecodingError` only when it is not an object.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      id = (try? container.decodeIfPresent(String.self, forKey: .id)) ?? ""
      reason = try? container.decodeIfPresent(String.self, forKey: .reason)
    }
  }
}
