/// An app a host session opened in the helper.
public struct HostSessionApp: Decodable, Equatable, Sendable {
  /// The app's name, such as "Notes".
  public var name: String
  /// Whether it opened a copy Atlas made of the app, rather than the app itself.
  public var copied: Bool

  /// Creates an app.
  ///
  /// - Parameters:
  ///   - name: The app's name.
  ///   - copied: Whether it opened a copy; not by default.
  public init(name: String, copied: Bool = false) {
    self.name = name
    self.copied = copied
  }

  /// The fields of one `apps` entry.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case name, copied
  }

  /// Decodes an app, reading a missing or mistyped `copied` as false.
  ///
  /// - Parameter decoder: The decoder reading one `apps` entry.
  /// - Throws: `DecodingError` when the entry is not an object or names no app, which drops it.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    guard let name = try? container.decodeIfPresent(String.self, forKey: .name), !name.isEmpty else {
      throw DecodingError.dataCorrupted(
        DecodingError.Context(codingPath: decoder.codingPath, debugDescription: "An app names no name")
      )
    }
    self.name = name
    copied = (try? container.decodeIfPresent(Bool.self, forKey: .copied)) == true
  }
}
