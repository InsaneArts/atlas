/// The desktop a host session acts on, as `sessions.json` names it once the service serves more than one: what the
/// app titles it by, and the ID a request for its Activity names it by.
public struct HostSessionDesktop: Decodable, Equatable, Sendable {
  /// Its ID, which a request names it by.
  public var id: String
  /// The name its records carry.
  public var key: String
  /// The settings' name for it.
  public var name: String
  /// What kind of desktop it is.
  public var kind: DesktopList.Kind

  /// Creates a desktop.
  ///
  /// - Parameters:
  ///   - id: Its ID.
  ///   - key: The name its records carry.
  ///   - name: The settings' name for it.
  ///   - kind: What kind of desktop it is.
  public init(id: String, key: String, name: String, kind: DesktopList.Kind) {
    self.id = id
    self.key = key
    self.name = name
    self.kind = kind
  }

  /// The fields of one desktop.
  private enum CodingKeys: String, CodingKey {
    case id, key, name, kind
  }

  /// Decodes a desktop: one without an ID is not one.
  ///
  /// - Parameter decoder: The decoder reading a session's `desktop`.
  /// - Throws: `DecodingError` when it is not an object or names no ID.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    id = try container.decode(String.self, forKey: .id)
    key = container.lenientText(.key) ?? ""
    name = container.lenientText(.name) ?? ""
    kind = DesktopList.Kind(word: container.lenientText(.kind) ?? "")
  }
}
