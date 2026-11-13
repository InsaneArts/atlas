extension ViewerStatus {
  /// A sign-in the service was holding when it ended: the helper's screen stays hidden from agents and their changes
  /// wait until the person finishes or cancels it.
  public struct InterruptedSignIn: Decodable, Equatable, Sendable {
    /// The site, when Atlas can still read it.
    public var site: String?
    /// What it was for, when Atlas can still read it.
    public var kind: SignInKind?
    /// Whether the person can say it is done: only when Atlas can still tell what it was and record it.
    public var canFinish: Bool
    /// What the app answers it by, once Atlas took it over; nil until then.
    public var id: String?

    /// Creates an interrupted sign-in.
    ///
    /// - Parameters:
    ///   - site: The site, when known.
    ///   - kind: What it was for, when known.
    ///   - canFinish: Whether the person can say it is done.
    ///   - id: What the app answers it by.
    public init(site: String? = nil, kind: SignInKind? = nil, canFinish: Bool = false, id: String? = nil) {
      self.site = site
      self.kind = kind
      self.canFinish = canFinish
      self.id = id
    }

    /// The fields of the object.
    enum CodingKeys: String, CodingKey {
      /// Each field by the name the service writes.
      case site, kind, canFinish, id
    }

    /// Decodes an interrupted sign-in, keeping nothing of a field that is missing or of the wrong type.
    ///
    /// - Parameter decoder: The decoder reading the object.
    /// - Throws: `DecodingError` only when it is not an object.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      site = try? container.decodeIfPresent(String.self, forKey: .site)
      kind = try? container.decodeIfPresent(SignInKind.self, forKey: .kind)
      canFinish = (try? container.decodeIfPresent(Bool.self, forKey: .canFinish)) == true
      id = try? container.decodeIfPresent(String.self, forKey: .id)
    }
  }
}
