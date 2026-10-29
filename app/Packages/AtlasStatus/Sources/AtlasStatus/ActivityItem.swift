import AtlasService

/// One row of recent activity: who did it and the service's record of what was done, which the app words.
public struct ActivityItem: Equatable, Sendable, Identifiable {
  /// Who did an action: the person through a viewer, or an agent.
  public enum Actor: Equatable, Sendable {
    /// The person, through a viewer.
    case you
    /// An agent.
    case agent
  }

  /// The row's id from the service, which holds across readings; its place in the list, newest first, from a service
  /// that gives none.
  public let id: String
  /// Who did it.
  public let actor: Actor
  /// What was done, as the service recorded it.
  public let row: ViewerStatus.Activity

  /// Whether it worked.
  public var ok: Bool { row.ok }
  /// Whether it failed: it did not work, and was not refused before anything was sent, as when another app was in
  /// front, which is no problem.
  public var failed: Bool { !row.ok && !row.nothingSent }
  /// Seconds ago, when known.
  public var age: Double? { row.ageSeconds }
  /// The key of the host session it came from; nil for the person's own and for commands no host named.
  public var session: String? { row.session }
  /// The key of the host session an agent's action came from: Terminal's empty key for one no host named.
  public var sessionKey: String { row.sessionKey }
}
