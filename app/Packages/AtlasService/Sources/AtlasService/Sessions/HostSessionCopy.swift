/// A copy of an app Atlas made for a host session. The service keeps no token or time for it.
public struct HostSessionCopy: Decodable, Equatable, Sendable {
  /// The copied app's name, such as "Notes"; empty when the service gives none.
  public var app: String

  /// Creates a copy.
  ///
  /// - Parameter app: The copied app's name.
  public init(app: String) {
    self.app = app
  }

  /// The fields of one `copies` entry.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case app
  }

  /// Decodes a copy, reading a missing or mistyped app as empty.
  ///
  /// - Parameter decoder: The decoder reading one `copies` entry.
  /// - Throws: `DecodingError` only when the entry is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    app = (try? container.decodeIfPresent(String.self, forKey: .app)) ?? ""
  }
}
