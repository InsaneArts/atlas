extension ActivityLog {
  /// The listed account an agent's action acted in, as a row names it: the name the person reads, such as
  /// `github.com` or an app's name, and the key the account is listed by.
  public struct Account: Decodable, Equatable, Hashable, Sendable {
    /// What the person reads: the site's domain, or the app's name.
    public var name: String
    /// Which listed account it is.
    public var key: AccountKey

    /// Creates an account as a row names it.
    ///
    /// - Parameters:
    ///   - name: What the person reads.
    ///   - key: Which listed account it is.
    public init(name: String, key: AccountKey) {
      self.name = name
      self.key = key
    }

    /// The fields of a row's account, as the service writes them.
    enum CodingKeys: String, CodingKey {
      /// Each field by the name the service writes.
      case name, key
    }

    /// Decodes a row's account; one without a name or a whole key is none, which the row reading it takes as no
    /// account.
    ///
    /// - Parameter decoder: The decoder reading one account.
    /// - Throws: `DecodingError` when its name is missing, empty or of the wrong type, or its key does not read.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      guard let name = container.lenientText(.name) else {
        throw DecodingError.dataCorruptedError(forKey: .name, in: container, debugDescription: "No account name")
      }
      let key = try container.decode(AccountKey.self, forKey: .key)
      guard !key.id.isEmpty else {
        throw DecodingError.dataCorruptedError(forKey: .key, in: container, debugDescription: "No account id")
      }
      self.name = name
      self.key = key
    }
  }
}
