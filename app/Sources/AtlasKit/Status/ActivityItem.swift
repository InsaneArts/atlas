/// One row of recent activity, as shown.
package struct ActivityItem: Equatable, Sendable, Identifiable {
  /// Who did an action: the person through a viewer, or an agent.
  package enum Actor: Equatable, Sendable {
    /// The person, through a viewer: "You".
    case you
    /// An agent: "Agent".
    case agent

    /// The actor in words, as a row starts.
    package var word: String {
      switch self {
      case .you: "You"
      case .agent: "Agent"
      }
    }
  }

  /// The row's id from the service, which holds across readings; its place in the list, newest first, from a service
  /// that gives none.
  package let id: String
  /// Who did it.
  package let actor: Actor
  /// What was done, in words.
  package let text: String
  /// How long ago, in words; empty when unknown.
  package let when: String
  /// Whether it worked.
  package let ok: Bool
  /// Seconds ago, when known.
  package let age: Double?

  /// Who did what, in one line.
  package var line: String { "\(actor.word) \(text)" }
}
