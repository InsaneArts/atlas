extension AgentsStatus {
  /// The agents that already know how to use Atlas, in the command's order; one this app does not know is passed
  /// over.
  public var connected: [AgentID] {
    grouped
      .filter { $0.connected == true }
      .compactMap { AgentID(rawValue: $0.agent) }
  }
}
