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
  /// Whether the current change removes connections.
  private(set) var isRemoving = false
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

  /// Connects each chosen agent's installed parts together.
  ///
  /// - Parameters:
  ///   - agents: The displayed agents.
  ///   - claudeParts: Claude's installed components, as last reported.
  /// - Returns: The command's outcome, or nil when an install already runs or none is named.
  func install(_ agents: [AgentID], claudeParts: [String]) async -> Outcome? {
    let parts = agents.flatMap { $0 == .claude ? claudeParts : [$0.rawValue] }
    guard !parts.isEmpty else { return nil }
    return await run(Set(agents.map(\.rawValue)), .changeAgentParts(parts, remove: false))
  }

  /// Disconnects every component in a row, holding the same exclusion as Connect.
  ///
  /// - Parameters:
  ///   - id: The displayed agent id.
  ///   - parts: Its CLI component ids.
  /// - Returns: The command's outcome, or nil while another change runs.
  func disconnect(_ id: String, parts: [String]) async -> Outcome? {
    guard !isBusy, !parts.isEmpty else { return nil }
    isRemoving = true
    defer { isRemoving = false }
    return await run([id], .changeAgentParts(parts, remove: true))
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
