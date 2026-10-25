import AtlasCommands
import AtlasUI
import Foundation

extension SettingsPresenter {
  /// The Agents pane: Claude and Codex, each connected, ready to connect, not installed, or
  /// saying why its last Connect failed; then each project taught.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The pane.
  static func agents(_ snapshot: SettingsSnapshot) -> SettingsAgentsState {
    let agents = snapshot.agents
    let installing = snapshot.facts.installing
    let failures = snapshot.session.connectFailures
    let plugins = agents.choices.map { choice in
      let parts = choice.checked ? choice.parts : [AgentID.claude.rawValue, AgentsStatus.claudeAppID]
      return SettingsAgentRow(
        id: choice.id.rawValue,
        title: Words.agentTitle(choice.id),
        detail: detail(
          choice.id == .claude ? Words.agentParts(parts) : SettingsWords.usesAtlas,
          absent: choice.checked && !choice.found && !choice.connected
        ),
        icon: AgentRowIcons.icon(choice.id.rawValue, snapshot.icons),
        connection: connection(
          id: choice.id.rawValue,
          checked: choice.checked,
          found: choice.found,
          connected: choice.connected,
          failed: agents.failed,
          installing: installing,
          isRemoving: snapshot.facts.isRemoving
        ),
        canDisconnect: choice.hasConnection && installing.isEmpty
      )
    }
    return SettingsAgentsState(
      agents: plugins.map { row in
        let reported = agents.choices.first { $0.id.rawValue == row.id }?.problem
        return failing(row, failures[row.id]?.message ?? reported)
      },
      projects: snapshot.agents.projects.map(project),
      projectsReading: snapshot.agents.projectsReading,
      notice: notice(snapshot, in: .agents, busy: !installing.isEmpty)
    )
  }

  /// Where one agent stands: being connected from any window, connected, ready to connect unless another install
  /// runs, or why it can't be.
  ///
  /// - Parameters:
  ///   - id: The agent's id.
  ///   - checked: Whether `atlas agents status` reported it.
  ///   - found: Whether it is on this Mac.
  ///   - connected: Whether it uses Atlas.
  ///   - failed: Whether the last reading failed.
  ///   - installing: The agents being connected now, from any window.
  ///   - isRemoving: Whether the shared change disconnects its agents.
  /// - Returns: Its row's right side.
  static func connection(
    id: String,
    checked: Bool,
    found: Bool,
    connected: Bool,
    failed: Bool,
    installing: Set<String>,
    isRemoving: Bool = false
  ) -> SettingsAgentRow.Connection {
    if installing.contains(id) { return isRemoving ? .note("Disconnecting…") : .connecting }
    if connected { return .connected }
    if !checked { return .note(failed ? SettingsWords.notChecked : SettingsWords.checking) }
    if !found { return .missing }
    return .connect(isEnabled: installing.isEmpty)
  }

  /// What an agent's row says under its name: what Atlas does in it, or that it is not installed.
  ///
  /// - Parameters:
  ///   - uses: What Atlas does in it.
  ///   - absent: Whether it was reported, is not on this Mac and is not connected.
  /// - Returns: The detail.
  static func detail(_ uses: String, absent: Bool) -> String {
    absent ? SettingsWords.notInstalled : uses
  }

  /// An agent's row with why its last change failed, unless it is connecting now: the reason in place
  /// of its detail, and Show File where the reason is a settings file that needs mending.
  ///
  /// - Parameters:
  ///   - row: The agent's row.
  ///   - failure: Why its last Connect failed, as the command said it; nil for none.
  /// - Returns: The row.
  static func failing(_ row: SettingsAgentRow, _ failure: String?) -> SettingsAgentRow {
    guard let failure, row.connection != .connecting else { return row }
    var failing = row
    failing.problem = failure.hasPrefix("Couldn't disconnect") ? failure : SettingsWords.notConnected(failure)
    failing.showsFile = SettingsWords.isBadSettingsFile(failure) && AgentFiles.knowsSettings(of: row.id)
    return failing
  }

  /// An agent's name by its id in `atlas agents` results.
  ///
  /// - Parameter id: The id, such as `claude` or `claude-app`.
  /// - Returns: "Claude", "Codex", "Claude app", or the id itself for one this app does not know.
  static func agentTitle(_ id: String) -> String {
    if let agent = AgentID(rawValue: id) { return Words.agentTitle(agent) }
    return id == AgentsStatus.claudeAppID ? SettingsWords.claudeApp : id
  }

  /// One project taught: its folder's name, and where it is with the home folder as `~`, or that its files no longer
  /// have the Atlas section.
  ///
  /// - Parameter project: The project as `atlas agents status` lists it.
  /// - Returns: Its row.
  static func project(_ project: AgentsStatus.Project) -> SettingsProjectRow {
    let name = projectName(project.path)
    let taught = project.taught ?? true
    return SettingsProjectRow(
      id: project.path,
      name: name,
      detail: taught ? (project.path as NSString).abbreviatingWithTildeInPath : SettingsWords.sectionGone,
      detailIsPath: taught,
      forgetLabel: SettingsWords.forgetLabel(name)
    )
  }

  /// A project's name: its folder's.
  ///
  /// - Parameter path: The folder's path.
  /// - Returns: The folder's name, such as "opie".
  static func projectName(_ path: String) -> String {
    URL(fileURLWithPath: path).lastPathComponent
  }
}
