extension AgentsStatus {
  /// The agents that already know how to use Atlas, in the command's order; one this app does not know is passed
  /// over.
  package var connected: [AgentID] {
    agents
      .filter { $0.connected == true }
      .compactMap { AgentID(rawValue: $0.agent) }
  }
}
