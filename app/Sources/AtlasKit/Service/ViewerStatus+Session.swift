extension ViewerStatus {
  /// The helper's login session, as far as the viewer needs it.
  package struct Session: Decodable, Equatable, Sendable {
    /// The session's user name, when known.
    package var userName: String?
    /// Whether the input service is ready for input.
    package var inputReady = false
    /// Whether the input service answers at all.
    package var inputReachable = false

    /// Creates a session with nothing known.
    package init() {}

    /// The fields of the `session` object this app reads.
    enum CodingKeys: String, CodingKey { case user, inputService }

    /// The session's user.
    struct User: Decodable { var name: String? }

    /// The input service, as the session reports it.
    struct Input: Decodable {
      var ready: Bool?
      var ok: Bool?
    }

    /// Decodes a session, reading nothing that is missing or of the wrong type.
    ///
    /// - Parameter decoder: The decoder reading the `session` object.
    /// - Throws: `DecodingError` only when it is not an object.
    package init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      userName = (try? container.decodeIfPresent(User.self, forKey: .user))?.name
      let input = try? container.decodeIfPresent(Input.self, forKey: .inputService)
      inputReady = input?.ready == true
      inputReachable = input?.ok == true
    }
  }
}
