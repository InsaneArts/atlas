import AtlasCommands

/// The agents screen's words and the last screen's: each agent's name and where it stands, an agent that did not
/// connect, and what setup did.
extension OnboardingWords {
  /// An agent's name.
  ///
  /// - Parameter agent: The agent.
  /// - Returns: "Claude Code" or "Codex".
  static func agentTitle(_ agent: AgentID) -> String {
    Words.agentTitle(agent)
  }

  /// An agent that already knows Atlas.
  static let agentConnected = "Connected"
  /// An agent installed and switched off: Continue leaves it as it is.
  static let agentFound = "Found"
  /// An agent installed and switched on: Continue connects it.
  static let agentGetsSkill = "Gets the Atlas skill"
  /// An agent not on this Mac.
  static let agentMissing = "Not on this Mac"
  /// An agent `atlas agents status` could not check.
  static let agentNotChecked = "Not checked yet"
  /// `atlas agents status` failed: the step can be skipped, and Atlas asks again.
  static let agentsCheckFailed =
    "Atlas couldn’t check your agents. Continue to skip this, or wait while it tries again."

  /// The done screen's lede, in three lines: where Atlas lives, what to ask it, and the one thing to do after a
  /// restart.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: What Atlas does from now on, and what to do after a restart.
  static func doneLede(_ helper: String) -> String {
    "Atlas lives in your menu bar. Ask your agent to check something on screen and watch it work there. After a restart, log in to \(named(helper)) once and Atlas does the rest."
  }
}
