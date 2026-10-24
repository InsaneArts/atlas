import AtlasCommands
import AtlasService
import Observation

/// Connecting agents to Atlas, for every window that does it: the setup window's agents step and Settings' Connect.
/// One install runs at a time, app-wide, since each rewrites the plugin's marketplace and the agents' own settings;
/// the agents it connects stay reserved until its command ends, whether or not the window that asked is still open.
/// Both windows read ``installing`` for their buttons. ``AppContainer`` builds the one instance and passes it to each.
@Observable
final class AgentInstalls {
  /// The agents being connected now, by their ids in `atlas agents` results; empty while none is.
  private(set) var installing: Set<String> = []
  /// The `atlas` command line the installs run.
  @ObservationIgnored private let commands: any CommandRunning

  /// Creates the coordinator.
  ///
  /// - Parameter commands: The `atlas` command line.
  init(commands: any CommandRunning) {
    self.commands = commands
  }

  /// Whether an install runs now, from any window.
  var isBusy: Bool { !installing.isEmpty }

  /// Connects Claude Code or Codex by name.
  ///
  /// - Parameter agents: The agents.
  /// - Returns: The command's outcome, or nil when an install already runs or none is named.
  func install(_ agents: [AgentID]) async -> Outcome? {
    await run(Set(agents.map(\.rawValue)), .installAgents(agents))
  }

  /// Connects the Claude app alone.
  ///
  /// - Returns: The command's outcome, or nil when an install already runs.
  func connectClaudeApp() async -> Outcome? {
    await run([AgentsStatus.claudeAppID], .connectClaudeApp)
  }

  /// Runs one install, holding its agents until its command ends.
  ///
  /// - Parameters:
  ///   - agents: The agents it connects, by id.
  ///   - action: The command.
  /// - Returns: Its outcome, or nil when an install already runs or no agent is named.
  private func run(_ agents: Set<String>, _ action: Action) async -> Outcome? {
    guard installing.isEmpty, !agents.isEmpty else { return nil }
    installing = agents
    defer { installing = [] }
    return await commands.run(action)
  }
}
