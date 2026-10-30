/// `apps.json`'s body: the apps open in the helper, front to back, none when the list is missing or of the wrong type,
/// and the host sessions that opened them.
public struct HelperAppList: Decodable, Equatable, Sendable {
  /// The apps, front to back.
  public var apps: [HelperApp]
  /// The host sessions the apps' origins name, by key.
  public var sessions: [String: HostSessionSummary]

  /// Creates a list.
  ///
  /// - Parameters:
  ///   - apps: The apps, front to back.
  ///   - sessions: The host sessions the apps' origins name, by key.
  public init(apps: [HelperApp] = [], sessions: [String: HostSessionSummary] = [:]) {
    self.apps = apps
    self.sessions = sessions
  }

  /// The body's fields this app reads.
  enum CodingKeys: String, CodingKey { case apps, sessions }

  /// Decodes the body, keeping no apps when the list is missing or of the wrong type, and each session on its own, so
  /// one odd one is dropped and the rest are kept.
  ///
  /// - Parameter decoder: The decoder reading `apps.json`.
  /// - Throws: `DecodingError` only when the body is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    apps = (try? container.decodeIfPresent([HelperApp].self, forKey: .apps)) ?? []
    sessions = HostSessionSummary.map(
      try? container.decodeIfPresent([String: Lenient<HostSessionSummary>].self, forKey: .sessions)
    )
  }
}
