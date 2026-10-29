/// A host session as `activity.json` and `screenshots.json` name it, for the keys their rows carry: enough to head
/// its rows once the status no longer lists it.
public struct HostSessionSummary: Decodable, Equatable, Sendable {
  /// The session's name, such as "Claude Code · understudy".
  public var name: String
  /// The app it runs in.
  public var host: SessionHost
  /// The host's name in the service's words; empty when it gives none.
  public var hostLabel: String

  /// Creates a summary.
  ///
  /// - Parameters:
  ///   - name: The session's name.
  ///   - host: The app it runs in.
  ///   - hostLabel: The host's name in the service's words.
  public init(name: String, host: SessionHost, hostLabel: String = "") {
    self.name = name
    self.host = host
    self.hostLabel = hostLabel
  }

  /// The fields of one entry of the `sessions` map this app reads.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case name, host, hostLabel
  }

  /// Decodes a summary, keeping the default of anything missing or of the wrong type.
  ///
  /// - Parameter decoder: The decoder reading one entry of the `sessions` map.
  /// - Throws: `DecodingError` when the entry is not an object or names no session, which drops it.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    guard let name = container.lenientText(.name) else {
      throw DecodingError.dataCorrupted(
        DecodingError.Context(codingPath: decoder.codingPath, debugDescription: "A session names no name")
      )
    }
    self.name = name
    host = SessionHost(word: container.lenientText(.host))
    hostLabel = container.lenientText(.hostLabel) ?? ""
  }

  /// The `sessions` map of a list reply, each entry decoded on its own, so one odd entry is dropped and the rest are
  /// kept.
  ///
  /// - Parameter value: The map as decoded, each entry on its own.
  /// - Returns: The summaries, by session key.
  static func map(_ value: [String: Lenient<HostSessionSummary>]?) -> [String: HostSessionSummary] {
    (value ?? [:])
      .compactMapValues(\.value)
  }
}
