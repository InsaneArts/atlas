import Foundation

/// What the person allowed for a task: Allow for This Task lets one agent session go on in one account, until the
/// session ends, the person ends it, or an hour has passed.
public struct AllowedGrant: Decodable, Equatable, Sendable, Identifiable {
  /// The key of the agent session it covers.
  public var session: String
  /// The account it covers, as the input service names it.
  public var account: AccountKey
  /// The account, as the person reads it, such as "GitHub".
  public var name: String
  /// When it ends at the latest.
  public var ends: Date

  /// One grant in a list: its session and its account together.
  public var id: String {
    [session, account.kind.rawValue, account.id, account.browser ?? ""].joined(separator: "\u{1F}")
  }

  /// Creates a grant.
  ///
  /// - Parameters:
  ///   - session: The key of the agent session it covers.
  ///   - account: The account it covers.
  ///   - name: The account, as the person reads it.
  ///   - ends: When it ends at the latest.
  public init(session: String, account: AccountKey, name: String, ends: Date) {
    self.session = session
    self.account = account
    self.name = name
    self.ends = ends
  }

  /// The fields of a grant, as the service writes them.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case session, account, name, ends
  }

  /// Decodes a grant.
  ///
  /// - Parameter decoder: The decoder reading one grant.
  /// - Throws: `DecodingError` when its account or its end is missing.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    session = container.lenientText(.session) ?? ""
    account = try container.decode(AccountKey.self, forKey: .account)
    name = container.lenientText(.name) ?? account.id
    guard let ends = container.lenientDate(milliseconds: .ends) else {
      throw DecodingError.dataCorruptedError(forKey: .ends, in: container, debugDescription: "No end")
    }
    self.ends = ends
  }
}
