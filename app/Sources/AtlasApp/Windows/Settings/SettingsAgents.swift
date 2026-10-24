import AtlasCommands
import AtlasOnboarding

/// The agents and projects as `atlas agents status` last reported them: Claude Code and Codex as the setup window
/// reads them, the Claude app's own row, and every project taught.
struct SettingsAgents {
  /// Claude Code and Codex, each with whether it is checked, found and connected.
  var choices = AgentChoice.all
  /// The Claude app's row, once reported.
  var app: AgentsStatus.Agent?
  /// Every project taught, in the command's order.
  var projects: [AgentsStatus.Project] = []
  /// Whether the last reading failed.
  var failed = false
}
