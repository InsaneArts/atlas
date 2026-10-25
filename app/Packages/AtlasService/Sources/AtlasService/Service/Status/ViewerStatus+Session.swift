extension ViewerStatus {
  /// The helper's login session, as far as the viewer needs it.
  public struct Session: Decodable, Equatable, Sendable {
    /// The session's user name, when known.
    public var userName: String?
    /// Whether the input service is ready for input.
    public var inputReady = false
    /// Whether the input service answers at all.
    public var inputReachable = false

    /// Creates a session with nothing known.
    init() {}

    /// The fields of the `session` object this app reads.
    enum CodingKeys: String, CodingKey { case user, inputService }

    /// The session's user.
    struct User: Decodable {
      /// The user's short name; nil when the session does not say.
      var name: String?
    }

    /// The input service, as the session reports it.
    struct Input: Decodable {
      /// Whether the input service is ready to send events.
      var ready: Bool?
      /// Whether the input service answered.
      var ok: Bool?
    }

    /// Decodes a session, reading nothing that is missing or of the wrong type.
    ///
    /// - Parameter decoder: The decoder reading the `session` object.
    /// - Throws: `DecodingError` only when it is not an object.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      userName = (try? container.decodeIfPresent(User.self, forKey: .user))?.name
      let input = try? container.decodeIfPresent(Input.self, forKey: .inputService)
      inputReady = input?.ready == true
      inputReachable = input?.ok == true
    }
  }
}
