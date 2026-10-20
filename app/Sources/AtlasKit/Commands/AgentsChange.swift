/// What `atlas agents install` reports: whether each agent was connected, and why not when it was not.
package struct AgentsChange: Decodable, Sendable {
  /// One agent's outcome.
  package struct Agent: Decodable, Sendable {
    /// The agent's id, such as `claude` or `codex`.
    package let agent: String
    /// Whether it was connected.
    package let ok: Bool?
    /// Why it was not, when it was not.
    package let error: String?
  }

  /// The agents, in the command's order.
  package let agents: [Agent]

  /// The agents it connected.
  package var connected: [AgentID] {
    agents
      .filter { $0.ok == true }
      .compactMap { AgentID(rawValue: $0.agent) }
  }

  /// The agents it could not connect, each with why, in the command's order; an agent this app does not know is
  /// left out.
  package var failures: [(agent: AgentID, error: String)] {
    agents
      .filter { $0.ok != true }
      .compactMap { row in
        AgentID(rawValue: row.agent)
          .map { ($0, row.error ?? "") }
      }
  }
}
