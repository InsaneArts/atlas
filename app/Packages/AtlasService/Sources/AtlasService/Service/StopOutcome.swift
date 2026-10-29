/// What the person's Stop came to: agents stopped, what the service warned of, and the background jobs it could not
/// end. A job is named here only when the helper's daemon answered for it.
public struct StopOutcome: Decodable, Equatable, Sendable {
  /// Whether agents are stopped.
  public var held = true
  /// What the service warned of, in its own words, such as a helper it could not reach to end its jobs.
  public var warnings: [String] = []
  /// The jobs nothing could be sent to: their processes no longer match them.
  public var refused: [Job] = []
  /// The jobs whose processes could not be read, so nothing was sent to them.
  public var unknown: [Job] = []
  /// The jobs whose records the helper cannot read, which may still run.
  public var unresolved: [Job] = []

  /// How many background jobs the Stop could not end.
  public var jobsLeft: Int { refused.count + unknown.count + unresolved.count }

  /// Whether every background job the helper knows of was told to end.
  public var isComplete: Bool { jobsLeft == 0 }

  /// Creates the outcome of a Stop that ended everything and warned of nothing.
  public init() {}

  /// Creates an outcome.
  ///
  /// - Parameters:
  ///   - warnings: What the service warned of.
  ///   - refused: The jobs nothing could be sent to.
  ///   - unknown: The jobs whose processes could not be read.
  ///   - unresolved: The jobs whose records cannot be read.
  public init(warnings: [String], refused: [Job] = [], unknown: [Job] = [], unresolved: [Job] = []) {
    self.warnings = warnings
    self.refused = refused
    self.unknown = unknown
    self.unresolved = unresolved
  }

  /// The fields of the Stop's reply.
  private enum CodingKeys: String, CodingKey {
    case held, warnings, jobs
  }

  /// The fields of its `jobs` object.
  private enum JobsKeys: String, CodingKey {
    case refused, unknown, unresolved
  }

  /// Decodes a Stop's reply, keeping the default of anything missing or of the wrong type.
  ///
  /// - Parameter decoder: The decoder reading the reply.
  /// - Throws: `DecodingError` only when it is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    held = (try? container.decodeIfPresent(Bool.self, forKey: .held)) ?? true
    warnings = (try? container.decodeIfPresent([String].self, forKey: .warnings)) ?? []
    guard let jobs = try? container.nestedContainer(keyedBy: JobsKeys.self, forKey: .jobs) else { return }
    refused = (try? jobs.decodeIfPresent([Job].self, forKey: .refused)) ?? []
    unknown = (try? jobs.decodeIfPresent([Job].self, forKey: .unknown)) ?? []
    unresolved = (try? jobs.decodeIfPresent([Job].self, forKey: .unresolved)) ?? []
  }
}
