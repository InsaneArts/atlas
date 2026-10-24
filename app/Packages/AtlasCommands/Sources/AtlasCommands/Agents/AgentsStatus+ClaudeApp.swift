extension AgentsStatus {
  /// The Claude app's id in `atlas agents` results. Shipped, so it never changes.
  public static let claudeAppID = "claude-app"

  /// The Claude app's component row, before the app groups it with Claude Code.
  public var claudeApp: Agent? {
    agents.first { $0.agent == Self.claudeAppID }
  }
}
