/// A process that left a stopped background job's group and still ran, which Atlas never signals: its command name, as
/// the helper's kernel keeps it.
public struct CleanupProcess: Decodable, Equatable, Sendable {
  /// Its command name, such as "sleep"; empty when the service gives none.
  public var name: String

  /// Creates a process.
  ///
  /// - Parameter name: Its command name.
  public init(name: String) {
    self.name = name
  }

  /// The fields of one `left` entry this app reads.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case name
  }

  /// Decodes a process, reading a missing or mistyped name as empty, so it still counts.
  ///
  /// - Parameter decoder: The decoder reading one `left` entry.
  /// - Throws: `DecodingError` only when the entry is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    name = container.lenientText(.name) ?? ""
  }
}
