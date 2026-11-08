/// One day of the Activity outline, owning its sessions; its heading shows only while the outline spans more than one
/// day.
public struct ViewerActivityDay: Equatable, Sendable, Identifiable {
  /// The day's identity, such as its first moment in seconds since 1970.
  public var id: String
  /// Its heading, such as "Today", "Yesterday" or "Mon 19 Oct"; empty for none.
  public var title: String
  /// Its sessions, newest first.
  public var sessions: [ViewerOutlineSession]
  /// The row that shows the rest of a long day's sessions; nil while every one shows.
  public var more: ViewerMoreSessions?

  /// Creates a day with its sessions.
  ///
  /// - Parameters:
  ///   - id: The day's identity.
  ///   - title: Its heading, or empty for none.
  ///   - sessions: Its sessions.
  ///   - more: The row for the rest of its sessions; none by default.
  public init(id: String, title: String, sessions: [ViewerOutlineSession], more: ViewerMoreSessions? = nil) {
    self.id = id
    self.title = title
    self.sessions = sessions
    self.more = more
  }
}
