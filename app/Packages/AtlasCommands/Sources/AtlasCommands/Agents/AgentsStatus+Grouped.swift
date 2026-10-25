extension AgentsStatus {
  /// The app's rows, grouping Claude Code and the Claude app while preserving the CLI's component ids.
  public var grouped: [Agent] {
    Self.grouped(agents)
  }

  /// Groups component status into one Claude row. Every installed part must be connected.
  ///
  /// - Parameter rows: The CLI's component rows.
  /// - Returns: Claude and Codex, when reported.
  public static func grouped(_ rows: [Agent]) -> [Agent] {
    AgentID.allCases
      .compactMap { id in
        let candidates = rows.filter {
          $0.agent == id.rawValue || (id == .claude && $0.agent == claudeAppID)
        }
        guard !candidates.isEmpty else { return nil }
        let applicable = candidates.filter { $0.installed == true || $0.connected == true }
        let errors = applicable.compactMap { row in
          row.error.map { "\(partTitle(row.agent)): \($0)" }
        }
        var agent = Agent(
          agent: id.rawValue,
          installed: !applicable.isEmpty,
          connected: !applicable.isEmpty && applicable.allSatisfy { $0.connected == true },
          error: errors.isEmpty ? nil : errors.joined(separator: "; ")
        )
        agent.parts = applicable.map(\.agent)
        if candidates.contains(where: { $0.connected == true }) {
          agent.hasConnection = true
        } else if candidates.allSatisfy({ $0.connected == false || $0.installed == false }) {
          agent.hasConnection = false
        }
        return agent
      }
  }

  /// The name of a connection's part, including in a failure.
  ///
  /// - Parameter id: The CLI component id.
  /// - Returns: Its plain name.
  public static func partTitle(_ id: String) -> String {
    switch id {
    case AgentID.claude.rawValue: "Claude Code"
    case claudeAppID: "The Claude app"
    case AgentID.codex.rawValue: "Codex"
    default: id
    }
  }
}
