/// What `atlas agents status` reports: which agents are installed and connected.
public struct AgentsStatus: Decodable, Sendable {
  /// One agent.
  public struct Agent: Decodable, Sendable {
    /// The agent's id, such as `claude` or `codex`.
    public let agent: String
    /// Whether it is installed on this Mac.
    public let installed: Bool?
    /// Whether it already knows how to use Atlas.
    public let connected: Bool?
    /// Why this part could not be checked.
    public let error: String?
    /// The installed or connected parts represented by this row.
    public var parts: [String]? = nil
    /// Whether at least one part is connected and can be disconnected.
    public var hasConnection: Bool? = nil
  }

  /// One project `atlas agents project` taught.
  public struct Project: Decodable, Sendable {
    /// The project's folder.
    public let path: String
    /// Whether its AGENTS.md still has the Atlas section.
    public let taught: Bool?
  }

  /// The agents, in the command's order.
  public let agents: [Agent]
  /// Every project taught, in the command's order; nil from a command line that lists none.
  public let projects: [Project]?
}
