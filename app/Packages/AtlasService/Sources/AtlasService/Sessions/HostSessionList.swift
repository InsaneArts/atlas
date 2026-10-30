/// `sessions.json`'s body: the host sessions live or ended within the last seven days, newest last seen first, and
/// every live session as the status lists them, however many. Every field may be missing, so each has a safe default.
public struct HostSessionList: Decodable, Equatable, Sendable {
  /// The sessions, newest last seen first; none when the list is missing or of the wrong type.
  public var sessions: [HostSessionRecord]
  /// Every live session in the status's shape and order, those on their first command with no record yet included;
  /// none from an older service, which does not send them.
  public var live: [ViewerStatus.HostSession]

  /// Creates a list.
  ///
  /// - Parameters:
  ///   - sessions: The sessions, newest last seen first.
  ///   - live: Every live session, in the status's order; none by default.
  public init(sessions: [HostSessionRecord] = [], live: [ViewerStatus.HostSession] = []) {
    self.sessions = sessions
    self.live = live
  }

  /// The body's fields this app reads.
  enum CodingKeys: String, CodingKey {
    /// The sessions.
    case sessions
    /// Every live session.
    case live
  }

  /// Decodes the body, each session on its own, so one odd entry is dropped and the rest are kept.
  ///
  /// - Parameter decoder: The decoder reading `sessions.json`.
  /// - Throws: `DecodingError` only when the body is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    sessions = container.lenientList(HostSessionRecord.self, forKey: .sessions)
    live = container.lenientList(ViewerStatus.HostSession.self, forKey: .live)
  }
}
