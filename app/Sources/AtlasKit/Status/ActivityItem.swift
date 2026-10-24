/// One row of recent activity: who did it and the service's record of what was done, which the app words.
package struct ActivityItem: Equatable, Sendable, Identifiable {
  /// Who did an action: the person through a viewer, or an agent.
  package enum Actor: Equatable, Sendable {
    /// The person, through a viewer.
    case you
    /// An agent.
    case agent
  }

  /// The row's id from the service, which holds across readings; its place in the list, newest first, from a service
  /// that gives none.
  package let id: String
  /// Who did it.
  package let actor: Actor
  /// What was done, as the service recorded it.
  package let row: ViewerStatus.Activity

  /// Whether it worked.
  package var ok: Bool { row.ok }
  /// Seconds ago, when known.
  package var age: Double? { row.ageSeconds }
}
