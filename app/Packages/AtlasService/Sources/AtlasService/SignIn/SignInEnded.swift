/// What Done said: what the hold was for, its site and browser, and, for a sign-in, whether the site joined the
/// accounts.
public struct SignInEnded: Decodable, Equatable, Sendable {
  /// What the hold was for.
  public var kind: SignInKind
  /// The site.
  public var site: String
  /// The browser, by its bundle identifier.
  public var browser: String?
  /// Whether the site joined the accounts, for a sign-in; nil for the others.
  public var added: Bool?

  /// Creates an ended sign-in.
  ///
  /// - Parameters:
  ///   - kind: What it was for.
  ///   - site: The site.
  ///   - browser: The browser; none by default.
  ///   - added: Whether the site joined the accounts; nil by default.
  public init(kind: SignInKind, site: String, browser: String? = nil, added: Bool? = nil) {
    self.kind = kind
    self.site = site
    self.browser = browser
    self.added = added
  }

  /// The fields of the reply.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case kind, site, browser, added
  }

  /// Decodes the reply, an `added` of another type read as unknown.
  ///
  /// - Parameter decoder: The decoder.
  /// - Throws: `DecodingError` when its kind or site is missing.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    kind = try container.decode(SignInKind.self, forKey: .kind)
    site = try container.decode(String.self, forKey: .site)
    browser = container.lenientText(.browser)
    added = container.lenientBool(.added)
  }
}
