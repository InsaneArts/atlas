extension ViewerStatus {
  /// The account macOS names at the person's screen.
  public struct ConsoleName: Decodable, Equatable, Sendable {
    /// Whether macOS names the person's account.
    public enum State: String, Equatable, Sendable {
      /// It names this account.
      case this
      /// It names another account.
      case another
      /// Atlas could not tell.
      case unknown
    }

    /// How the account stands; unknown by default.
    public var state: State
    /// The other account's name, when known.
    public var name: String?

    /// Creates the named account.
    ///
    /// - Parameters:
    ///   - state: How it stands; unknown by default.
    ///   - name: The other account's name, when known.
    public init(state: State = .unknown, name: String? = nil) {
      self.state = state
      self.name = name
    }

    /// The fields of the named account.
    enum CodingKeys: String, CodingKey { case state, name }

    /// Decodes the account, keeping defaults for missing or malformed fields.
    ///
    /// - Parameter decoder: The decoder reading the account.
    /// - Throws: A decoding error when the account is not an object.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      let word = try? container.decodeIfPresent(String.self, forKey: .state)
      state = word.flatMap(State.init(rawValue:)) ?? .unknown
      name = try? container.decodeIfPresent(String.self, forKey: .name)
    }
  }
}
