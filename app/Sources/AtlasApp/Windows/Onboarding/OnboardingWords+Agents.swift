import AtlasKit

/// The agents screen's words and the last screen's: each agent's name and state, an agent that did not connect, and
/// what setup did.
extension OnboardingWords {
  /// An agent's name.
  ///
  /// - Parameter agent: The agent.
  /// - Returns: "Claude Code" or "Codex".
  static func agentTitle(_ agent: AgentID) -> String {
    Words.agentTitle(agent)
  }

  /// An agent that already knows Atlas.
  static let agentConnected = "Connected. Every session has the Atlas skill."
  /// An agent installed and not yet connected.
  static let agentFound = "Found. Adds the Atlas skill to every session."
  /// An agent not on this Mac.
  static let agentMissing = "Not installed on this Mac."
  /// An agent while `atlas agents status` runs.
  static let agentChecking = "Checking…"
  /// An agent `atlas agents status` could not check.
  static let agentNotChecked = "Not checked yet."
  /// `atlas agents status` failed: the step can be skipped, and Atlas asks again.
  static let agentsCheckFailed =
    "Atlas couldn't check your agents. Continue to skip this, or wait while it tries again."

  /// The done screen's lede.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: What Atlas does from now on, and what to do after a restart.
  static func doneLede(_ helper: String) -> String {
    "That was the whole setup. Atlas lives in your menu bar: ask your agent to check something on screen, and watch it work from there. After a restart, just log in to \(named(helper)) again; Atlas does the rest. Atlas doesn't need administrator access again. Removing it later asks once more, to stop it in the helper."
  }
}
