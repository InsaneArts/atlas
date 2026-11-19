/// What the person's Stop came to: agents stopped, what the service warned of, and the background jobs it could not
/// end. A job is named here only when the helper's daemon answered for it. A Stop holds every desktop the service
/// serves: with more than one, what each said is read together, and each one is named by whether its Stop held, failed
/// or is not certain.
public struct StopOutcome: Decodable, Equatable, Sendable {
  /// Whether agents are stopped.
  public var held = true
  /// The desktops whose Stop held their agents for certain, by name.
  public var stopped: [String] = []
  /// The desktops whose Stop failed, by name. Each is uncertain too.
  public var failed: [String] = []
  /// The desktops whose Stop is not certain, by name: it failed, the desktop may not have been told, or it did not
  /// say its agents are held. Agents there are held all the same where Atlas could keep the Stop, as its warnings say.
  public var uncertain: [String] = []
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

  /// Whether the reply named what each desktop's Stop came to: every desktop it named is stopped or uncertain.
  var namesEachDesktop: Bool { !stopped.isEmpty || !uncertain.isEmpty }

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
    case held, warnings, jobs, desktops
  }

  /// The fields of its `jobs` object.
  private enum JobsKeys: String, CodingKey {
    case refused, unknown, unresolved
  }

  /// Decodes a Stop's reply, keeping the default of anything missing or of the wrong type. Each warning and job
  /// decodes on its own: an odd warning is dropped, and an odd job still counts, with an empty ID. A reply naming
  /// each desktop's own is read from those, in their order, whatever its first desktop said at its top.
  ///
  /// - Parameter decoder: The decoder reading the reply.
  /// - Throws: `DecodingError` only when it is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    let each = container.lenientList(Desktop.self, forKey: .desktops)
    guard each.isEmpty else {
      self = StopOutcome(each)
      return
    }
    held = container.lenientBool(.held) ?? true
    warnings = container.lenientList(String.self, forKey: .warnings)
    guard let jobs = try? container.nestedContainer(keyedBy: JobsKeys.self, forKey: .jobs) else { return }
    let listed = { (key: JobsKeys) in
      (try? jobs.decodeIfPresent([Lenient<Job>].self, forKey: key))?
        .map { $0.value ?? Job(id: "") } ?? []
    }
    refused = listed(.refused)
    unknown = listed(.unknown)
    unresolved = listed(.unresolved)
  }

  /// The outcome of a Stop of every desktop in `each`: held only where each is, every warning once, every job left,
  /// and each desktop by whether its Stop held, failed or is not certain.
  ///
  /// - Parameter each: What each desktop's Stop came to, in the service's order.
  init(_ each: [Desktop]) {
    held = each.allSatisfy(\.reply.held)
    for desktop in each {
      warnings += desktop.reply.warnings.filter { !warnings.contains($0) }
      refused += desktop.reply.refused
      unknown += desktop.reply.unknown
      unresolved += desktop.reply.unresolved
    }
    stopped = each.filter(\.isStopped)
      .map(\.name)
    failed = each.filter(\.failed)
      .map(\.name)
    uncertain = each.filter { !$0.isStopped }
      .map(\.name)
  }
}
