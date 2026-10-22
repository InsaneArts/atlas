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

  /// One project `atlas agents project` taught.
  package struct Project: Decodable, Sendable {
    /// The project's folder.
    package let path: String
    /// Whether its AGENTS.md still has the Atlas section.
    package let taught: Bool?
  }

  /// The agents, in the command's order.
  package let agents: [Agent]
  /// Every project taught, in the command's order; nil from a command line that lists none.
  package let projects: [Project]?
}
