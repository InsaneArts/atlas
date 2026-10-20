/// The viewer service's `status.json`: the helper's readiness, its session
/// and the overview the panel and viewer show. Every field may be missing
/// (the service could not reach the helper), so each has a safe default.
package struct ViewerStatus: Decodable, Equatable, Sendable {
  /// Whether every readiness check passed.
  package var ready = false
  /// Whether the helper's desktop can be watched.
  package var canObserve = false
  /// The readiness checks, in the service's order.
  package var checks: [Check] = []
  /// What to do next, in the service's words.
  package var nextSteps: [String] = []
  /// Why the service could not read the helper, when it could not.
  package var error: String?
  /// The helper's login session.
  package var session = Session()
  /// What the helper has shown it can do.
  package var capabilities = Capabilities()
  /// What is happening in the helper, summed up.
  package var overview = Overview()

  /// Creates a status with every field at its safe default.
  package init() {}

  /// The root fields of `status.json`.
  enum CodingKeys: String, CodingKey {
    case ready, canObserve, checks, nextSteps, error, session, capabilities, overview
  }

  /// Decodes a status, keeping the default of every field that is missing or of the wrong type.
  ///
  /// - Parameter decoder: The decoder reading `status.json`.
  /// - Throws: `DecodingError` only when the reply is not an object.
  package init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    ready = (try? container.decodeIfPresent(Bool.self, forKey: .ready)) == true
    canObserve = (try? container.decodeIfPresent(Bool.self, forKey: .canObserve)) == true
    checks = (try? container.decodeIfPresent([Check].self, forKey: .checks)) ?? []
    nextSteps = (try? container.decodeIfPresent([String].self, forKey: .nextSteps)) ?? []
    error = try? container.decodeIfPresent(String.self, forKey: .error)
    session = (try? container.decodeIfPresent(Session.self, forKey: .session)) ?? Session()
    capabilities = (try? container.decodeIfPresent(Capabilities.self, forKey: .capabilities)) ?? Capabilities()
    overview = (try? container.decodeIfPresent(Overview.self, forKey: .overview)) ?? Overview()
  }
}
