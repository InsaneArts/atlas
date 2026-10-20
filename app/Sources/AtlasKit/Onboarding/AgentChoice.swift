/// One agent on the agents screen, as `atlas agents status` reported it and the person switched it.
package struct AgentChoice: Equatable, Sendable {
  /// Which agent.
  package let id: AgentID
  /// Whether it is installed on this Mac.
  package var found = false
  /// Whether it already knows how to use Atlas.
  package var connected = false
  /// Whether the person wants it connected.
  package var on = false

  /// Creates an agent nothing is known of yet.
  ///
  /// - Parameter id: Which agent.
  package init(id: AgentID) {
    self.id = id
  }
}
