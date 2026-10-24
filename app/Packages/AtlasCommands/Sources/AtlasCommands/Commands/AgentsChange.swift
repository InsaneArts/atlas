/// What `atlas agents install` reports: whether each agent was connected, and why not when it was not.
public struct AgentsChange: Decodable, Sendable {
  /// One agent's outcome.
  public struct Agent: Decodable, Sendable {
    /// The agent's id, such as `claude` or `codex`.
    public let agent: String
    /// Whether it was connected.
    public let ok: Bool?
    /// Why it was not, when it was not.
    public let error: String?
  }

  /// The agents, in the command's order.
  public let agents: [Agent]

  /// The agents it connected.
  public var connected: [AgentID] {
    AgentID.allCases
      .filter { id in
        let parts = agents.filter { $0.agent == id.rawValue || (id == .claude && $0.agent == AgentsStatus.claudeAppID) }
        return !parts.isEmpty && parts.allSatisfy { $0.ok == true }
      }
  }

  /// The agents it could not connect, each with why, in the command's order; an agent this app does not know is
  /// left out.
  public var failures: [(agent: AgentID, error: String)] {
    agents
      .filter { $0.ok != true }
      .compactMap { row in
        AgentID(rawValue: row.agent == AgentsStatus.claudeAppID ? AgentID.claude.rawValue : row.agent)
          .map { id in
            let reason = row.error ?? "it didn't connect"
            return (id, id == .claude ? "\(AgentsStatus.partTitle(row.agent)): \(reason)" : reason)
          }
      }
  }
}
