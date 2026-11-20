/// `sessions.json`'s body: the host sessions live or ended within the last seven days, newest last seen first, and
/// every currently relevant session as the status lists them, however many. Every field may be missing, so each has a safe default.
public struct HostSessionList: Decodable, Equatable, Sendable {
  /// The sessions, newest last seen first; none when the list is missing or of the wrong type.
  public var sessions: [HostSessionRecord]
  /// The wire list named `live` holds relevant sessions, including ended recovery attention or proven work;
  /// each entry's lifetime flags remain independent. Sessions on their first command may have no record;
  /// none from an older service, which does not send them. Once the service serves more than one desktop, each names
  /// the desktop it acts on, the viewed desktop's first, then every other desktop's.
  public var live: [ViewerStatus.HostSession]
  /// The ID of the desktop viewed when it was listed, once the service serves more than one; nil otherwise.
  public var viewed: String?

  /// Creates a list.
  ///
  /// - Parameters:
  ///   - sessions: The sessions, newest last seen first.
  ///   - live: Every relevant session, in the status's order; none by default.
  ///   - viewed: The ID of the desktop viewed, once there is more than one; none by default.
  public init(sessions: [HostSessionRecord] = [], live: [ViewerStatus.HostSession] = [], viewed: String? = nil) {
    self.sessions = sessions
    self.live = live
    self.viewed = viewed
  }

  /// The relevant sessions on the desktop viewed, in the list's order: every one while one desktop is served.
  public var onViewed: [ViewerStatus.HostSession] {
    guard let viewed else { return live }
    return live.filter { $0.desktop.map { $0.id == viewed } ?? true }
  }

  /// The relevant sessions on a desktop other than the one viewed, in the list's order; none while one is served.
  public var elsewhere: [ViewerStatus.HostSession] {
    guard let viewed else { return [] }
    return live.filter { $0.desktop.map { $0.id != viewed } ?? false }
  }

  /// The body's fields this app reads.
  enum CodingKeys: String, CodingKey {
    /// The sessions.
    case sessions
    /// Every relevant session.
    case live
    /// The desktop viewed.
    case viewed
  }

  /// Decodes the body, each session on its own, so one odd entry is dropped and the rest are kept.
  ///
  /// - Parameter decoder: The decoder reading `sessions.json`.
  /// - Throws: `DecodingError` only when the body is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    sessions = container.lenientList(HostSessionRecord.self, forKey: .sessions)
    live = container.lenientList(ViewerStatus.HostSession.self, forKey: .live)
    viewed = container.lenientText(.viewed)
  }
}
