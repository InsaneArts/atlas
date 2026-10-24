import AtlasCommands
import AtlasOnboarding
import AtlasUI

/// The grouped agents and projects as `atlas agents status` last reported them.
struct SettingsAgents {
  /// Claude and Codex, each with their installed parts and connection state.
  var choices = AgentChoice.all
  /// Every project taught, in the command's order.
  var projects: [AgentsStatus.Project] = []
  /// Whether projects are being read, loaded or retained after a failed reading.
  var projectsReading: SettingsProjectsReading = .reading
  /// Whether the last reading failed.
  var failed = false
}
