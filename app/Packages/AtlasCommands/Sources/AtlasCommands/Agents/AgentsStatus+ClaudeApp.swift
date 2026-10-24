extension AgentsStatus {
  /// The Claude app's id in `atlas agents` results. Shipped, so it never changes.
  public static let claudeAppID = "claude-app"

  /// The Claude app's row, when the command reported one: the app has no plugin command line, so it is no
  /// ``AgentID`` and `atlas agents install` connects it only when no agent is named.
  public var claudeApp: Agent? {
    agents.first { $0.agent == Self.claudeAppID }
  }
}
