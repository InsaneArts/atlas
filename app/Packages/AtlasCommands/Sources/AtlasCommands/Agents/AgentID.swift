/// An agent Atlas can connect, as `atlas agents` names it. Shipped names, so they never change.
public enum AgentID: String, CaseIterable, Sendable {
  /// Claude, grouping Claude Code and the Claude app in the app's lists.
  case claude
  /// Codex.
  case codex
  /// opencode.
  case opencode
  /// Pi.
  case pi
}
