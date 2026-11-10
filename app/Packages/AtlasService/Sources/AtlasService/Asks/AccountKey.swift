/// Which account a question or a grant is about, as the input service names it: a site in a browser, or an app. Names
/// are only for the person to read; grants go by this.
public struct AccountKey: Codable, Equatable, Hashable, Sendable {
  /// A site or an app.
  public var kind: ViewerStatus.Account.Kind
  /// The site's domain, or the app's bundle identifier.
  public var id: String
  /// For a site, the browser it was seen in.
  public var browser: String?

  /// Creates an account's key.
  ///
  /// - Parameters:
  ///   - kind: A site or an app.
  ///   - id: The site's domain, or the app's bundle identifier.
  ///   - browser: For a site, the browser it was seen in.
  public init(kind: ViewerStatus.Account.Kind, id: String, browser: String? = nil) {
    self.kind = kind
    self.id = id
    self.browser = browser
  }

  /// The fields of a key, as the service writes them.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case kind, id, browser
  }

  /// Decodes a key.
  ///
  /// - Parameter decoder: The decoder reading one key.
  /// - Throws: `DecodingError` when its kind or id is missing.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    kind = try container.decode(ViewerStatus.Account.Kind.self, forKey: .kind)
    id = try container.decode(String.self, forKey: .id)
    browser = try? container.decodeIfPresent(String.self, forKey: .browser)
  }

  /// Encodes the key as the service wrote it, a missing browser as null, so the service finds the very account it
  /// named.
  ///
  /// - Parameter encoder: The encoder.
  /// - Throws: The encoder's error.
  public func encode(to encoder: any Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(kind, forKey: .kind)
    try container.encode(id, forKey: .id)
    try container.encode(browser, forKey: .browser)
  }
}
