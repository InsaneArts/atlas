extension ViewerStatus {
  /// One account Atlas lists for the helper: a site by its registrable domain, or an app by its bundle identifier, as
  /// the person's viewer reads them from Atlas's record. It holds no user name or anything typed.
  public struct Account: Decodable, Equatable, Sendable, Identifiable {
    /// What an account is.
    public enum Kind: String, Codable, Hashable, Sendable {
      /// A web site.
      case site
      /// A Mac app.
      case app
    }

    /// A site or an app.
    public var kind: Kind
    /// The site's domain, or the app's bundle identifier.
    public var key: String
    /// What the person reads: the domain, or the app's name.
    public var name: String
    /// For a site, the bundle identifier of the browser it was seen in.
    public var browser: String?
    /// When it was added, in milliseconds since 1970.
    public var added: Double
    /// Whether Atlas added it after the person typed a password there, rather than the person by hand.
    public var addedAfterPassword: Bool
    /// How freely agents act there: Ask Before Sending unless the person chose another.
    public var level: AccountLevel

    /// One account in the list: its kind, key and browser together.
    public var id: String { [kind.rawValue, key, browser ?? ""].joined(separator: "\u{1F}") }

    /// Creates an account.
    ///
    /// - Parameters:
    ///   - kind: A site or an app.
    ///   - key: The site's domain, or the app's bundle identifier.
    ///   - name: What the person reads.
    ///   - browser: For a site, the browser it was seen in.
    ///   - added: When it was added, in milliseconds since 1970.
    ///   - addedAfterPassword: Whether Atlas added it after the person typed a password there.
    ///   - level: How freely agents act there; Ask Before Sending by default.
    public init(
      kind: Kind,
      key: String,
      name: String,
      browser: String? = nil,
      added: Double,
      addedAfterPassword: Bool = false,
      level: AccountLevel = .ask
    ) {
      self.kind = kind
      self.key = key
      self.name = name
      self.browser = browser
      self.added = added
      self.addedAfterPassword = addedAfterPassword
      self.level = level
    }

    /// The fields of an account, as the service writes them.
    enum CodingKeys: String, CodingKey {
      /// Each field by the name the service writes.
      case kind, id, name, browser, added, how, level
    }

    /// Decodes an account; the service's `how` says whether a password added it, and a level it doesn't know reads as
    /// Ask Before Sending.
    ///
    /// - Parameter decoder: The decoder reading one account.
    /// - Throws: `DecodingError` when its kind, id, name or time is missing.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      kind = try container.decode(Kind.self, forKey: .kind)
      key = try container.decode(String.self, forKey: .id)
      name = try container.decode(String.self, forKey: .name)
      browser = try? container.decodeIfPresent(String.self, forKey: .browser)
      added = try container.decode(Double.self, forKey: .added)
      addedAfterPassword = (try? container.decodeIfPresent(String.self, forKey: .how)) == "takeControl"
      level = (try? container.decodeIfPresent(AccountLevel.self, forKey: .level)) ?? .ask
    }
  }
}
