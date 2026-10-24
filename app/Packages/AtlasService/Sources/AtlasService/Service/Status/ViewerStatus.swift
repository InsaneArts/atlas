/// The viewer service's `status.json`: the helper's readiness, its session
/// and the overview the panel and viewer show. Every field may be missing
/// (the service could not reach the helper), so each has a safe default.
public struct ViewerStatus: Decodable, Equatable, Sendable {
  /// Whether every readiness check passed.
  public var ready = false
  /// Whether the helper's desktop can be watched.
  var canObserve = false
  /// The readiness checks, in the service's order.
  public var checks: [Check] = []
  /// What to do next, in the service's words.
  var nextSteps: [String] = []
  /// Why the service could not read the helper, when it could not.
  var error: String?
  /// Whether a viewer holds mouse and keyboard control, as the service's lease says; nil when it does not say.
  var controlActive: Bool?
  /// The helper's login session.
  public var session = Session()
  /// What the helper has shown it can do.
  public var capabilities = Capabilities()
  /// What is happening in the helper, summed up.
  public var overview = Overview()

  /// Creates a status with every field at its safe default.
  public init() {}

  /// The root fields of `status.json`.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case ready, canObserve, checks, nextSteps, error, controlActive, session, capabilities, overview
  }

  /// Decodes a status, keeping the default of every field that is missing or of the wrong type.
  ///
  /// - Parameter decoder: The decoder reading `status.json`.
  /// - Throws: `DecodingError` only when the reply is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    ready = (try? container.decodeIfPresent(Bool.self, forKey: .ready)) == true
    canObserve = (try? container.decodeIfPresent(Bool.self, forKey: .canObserve)) == true
    checks = (try? container.decodeIfPresent([Check].self, forKey: .checks)) ?? []
    nextSteps = (try? container.decodeIfPresent([String].self, forKey: .nextSteps)) ?? []
    error = try? container.decodeIfPresent(String.self, forKey: .error)
    controlActive = try? container.decodeIfPresent(Bool.self, forKey: .controlActive)
    session = (try? container.decodeIfPresent(Session.self, forKey: .session)) ?? Session()
    capabilities = (try? container.decodeIfPresent(Capabilities.self, forKey: .capabilities)) ?? Capabilities()
    overview = (try? container.decodeIfPresent(Overview.self, forKey: .overview)) ?? Overview()
  }
}
