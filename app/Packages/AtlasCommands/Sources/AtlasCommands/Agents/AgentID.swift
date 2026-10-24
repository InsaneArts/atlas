/// An agent Atlas can connect, as `atlas agents` names it. Shipped names, so they never change.
public enum AgentID: String, CaseIterable, Sendable {
  /// Claude Code.
  case claude
  /// Codex.
  case codex
}
