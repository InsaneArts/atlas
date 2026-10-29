import AtlasService
import Foundation

/// What the window last read of the host sessions' list, `sessions.json`: the sessions live or ended within the last
/// seven days and what each made, which the sidebar's Ended part, the sessions' facts and the Screenshots filter read. The
/// live sessions' state always comes from the status, which is fresher.
struct ViewerSessionsFacts {
  /// The list, newest last seen first; nil before the first reading.
  var list: HostSessionList?
  /// Why the last reading failed, while it has, which each session's facts say; nil once one worked.
  var failure: String?
  /// When the service last answered that it offers no list, as an older one does; nil while it offers one.
  var notOfferedAt: Date?

  /// 60 s: how long a service that offers no list is left before it is asked again: one updated meanwhile shows its
  /// sessions within a minute, and an older one is asked once a minute at most.
  static let askAgainAfter: TimeInterval = 60

  /// Whether to ask for the list now: always while the service offers it, else once ``askAgainAfter`` has passed
  /// since it last said it does not.
  ///
  /// - Parameter now: The time now.
  /// - Returns: Whether to ask.
  func asks(at now: Date) -> Bool {
    notOfferedAt.map { now.timeIntervalSince($0) >= Self.askAgainAfter } ?? true
  }

  /// A live session as the list has it, in the status's shape: one the status had no room for, say.
  ///
  /// - Parameter key: The session's key; empty for Terminal.
  /// - Returns: The session, or nil when the list does not name it live or was not read.
  func live(_ key: String) -> ViewerStatus.HostSession? {
    list?.live.first { $0.key == key }
  }

  /// A session as the list has it.
  ///
  /// - Parameter key: The session's key; empty for Terminal.
  /// - Returns: The session, or nil when the list does not name it or was not read.
  func record(_ key: String) -> HostSessionRecord? {
    list?.sessions.first { $0.key == key }
  }
}
