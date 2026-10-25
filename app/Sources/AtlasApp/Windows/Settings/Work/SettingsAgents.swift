import AtlasCommands
import AtlasOnboarding

/// The grouped agents and projects as `atlas agents status` last reported them.
struct SettingsAgents {
  /// Claude and Codex, each with their installed parts and connection state.
  var choices = AgentChoice.all
  /// Every project taught, in the command's order.
  var projects: [AgentsStatus.Project] = []
  /// Whether the last reading failed.
  var failed = false
}
