/// What `atlas agents status` reports: which agents are installed and connected.
package struct AgentsStatus: Decodable, Sendable {
  /// One agent.
  package struct Agent: Decodable, Sendable {
    /// The agent's id, such as `claude` or `codex`.
    package let agent: String
    /// Whether it is installed on this Mac.
    package let installed: Bool?
    /// Whether it already knows how to use Atlas.
    package let connected: Bool?
  }

  /// The agents, in the command's order.
  package let agents: [Agent]
}
