/// What `asks.json` says: the questions up now, oldest first, with what each would send, and what the person allowed.
public struct AskList: Decodable, Equatable, Sendable {
  /// The questions up now, oldest first.
  public var asks: [Ask]
  /// What the person allowed for a task.
  public var allowed: [AllowedGrant]

  /// Creates a list.
  ///
  /// - Parameters:
  ///   - asks: The questions up now, oldest first; none by default.
  ///   - allowed: What the person allowed; nothing by default.
  public init(asks: [Ask] = [], allowed: [AllowedGrant] = []) {
    self.asks = asks
    self.allowed = allowed
  }

  /// The fields of the reply.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case asks, allowed
  }

  /// Decodes the reply, each question and grant on its own, so one odd entry is dropped and the rest are kept.
  ///
  /// - Parameter decoder: The decoder reading the reply.
  /// - Throws: `DecodingError` only when it is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    asks = container.lenientList(Ask.self, forKey: .asks)
    allowed = container.lenientList(AllowedGrant.self, forKey: .allowed)
  }
}
