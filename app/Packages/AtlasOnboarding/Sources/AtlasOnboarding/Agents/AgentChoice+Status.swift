import AtlasCommands

extension AgentChoice {
  /// Every agent Atlas can connect, in the screen's order, before `atlas agents status` has said anything.
  public static var all: [AgentChoice] {
    AgentID.allCases
      .map(AgentChoice.init(id:))
  }

  /// The agents with what `atlas agents status` reported: each found one is switched on, ready to connect. A row
  /// for an agent this app does not know is passed over.
  ///
  /// - Parameters:
  ///   - agents: The agents as listed now.
  ///   - rows: What `atlas agents status` reported, one row an agent.
  /// - Returns: The agents, in the same order.
  public static func merged(_ agents: [AgentChoice], with rows: [AgentsStatus.Agent]) -> [AgentChoice] {
    var merged = agents
    for row in AgentsStatus.grouped(rows) {
      guard let index = merged.firstIndex(where: { $0.id.rawValue == row.agent }) else { continue }
      merged[index].parts = row.parts ?? []
      merged[index].hasConnection = row.hasConnection == true
      merged[index].problem = row.error
      merged[index].checked = true
      merged[index].found = row.installed == true
      merged[index].connected = row.connected == true
      merged[index].on = merged[index].found
    }
    return merged
  }

  /// The agents Continue installs: switched on, found on this Mac and not connected already.
  ///
  /// - Parameter agents: The agents as listed now.
  /// - Returns: Their ids, in order.
  public static func toConnect(_ agents: [AgentChoice]) -> [AgentID] {
    agents
      .filter { $0.on && $0.found && !$0.connected }
      .map(\.id)
  }
}
