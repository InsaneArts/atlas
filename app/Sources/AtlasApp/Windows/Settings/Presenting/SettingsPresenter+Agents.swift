import AtlasCommands
import AtlasUI
import Foundation

extension SettingsPresenter {
  /// The Agents pane: Claude Code, Codex and the Claude app, each connected, ready to connect, not installed, or
  /// saying why its last Connect failed; then each project taught.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The pane.
  static func agents(_ snapshot: SettingsSnapshot) -> SettingsAgentsState {
    let agents = snapshot.agents
    let installing = snapshot.facts.installing
    let failures = snapshot.session.connectFailures
    let plugins = agents.choices.map { choice in
      SettingsAgentRow(
        id: choice.id.rawValue,
        title: Words.agentTitle(choice.id),
        detail: detail(SettingsWords.usesAtlas, absent: choice.checked && !choice.found && !choice.connected),
        icon: AgentRowIcons.icon(choice.id.rawValue, snapshot.icons),
        connection: connection(
          id: choice.id.rawValue,
          checked: choice.checked,
          found: choice.found,
          connected: choice.connected,
          failed: agents.failed,
          installing: installing
        )
      )
    }
    let app = SettingsAgentRow(
      id: AgentsStatus.claudeAppID,
      title: SettingsWords.claudeApp,
      detail: detail(
        SettingsWords.showsHelper,
        absent: agents.app.map { $0.installed != true && $0.connected != true } ?? false
      ),
      icon: AgentRowIcons.icon(AgentsStatus.claudeAppID, snapshot.icons),
      connection: connection(
        id: AgentsStatus.claudeAppID,
        checked: agents.app != nil,
        found: agents.app?.installed == true,
        connected: agents.app?.connected == true,
        failed: agents.failed,
        installing: installing
      )
    )
    return SettingsAgentsState(
      agents: (plugins + [app]).map { failing($0, failures[$0.id]) },
      projects: snapshot.agents.projects.map(project),
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
  /// - Returns: Its row's right side.
  static func connection(
    id: String,
    checked: Bool,
    found: Bool,
    connected: Bool,
    failed: Bool,
    installing: Set<String>
  ) -> SettingsAgentRow.Connection {
    if installing.contains(id) { return .connecting }
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

  /// An agent's row with why its last Connect failed, unless it is connected or connecting now: the reason in place
  /// of its detail, and Show File where the reason is a settings file that needs mending.
  ///
  /// - Parameters:
  ///   - row: The agent's row.
  ///   - failure: Why its last Connect failed, as the command said it; nil for none.
  /// - Returns: The row.
  static func failing(_ row: SettingsAgentRow, _ failure: String?) -> SettingsAgentRow {
    guard let failure, row.connection != .connected, row.connection != .connecting else { return row }
    var failing = row
    failing.problem = SettingsWords.notConnected(failure)
    failing.showsFile = SettingsWords.isBadSettingsFile(failure) && AgentFiles.knowsSettings(of: row.id)
    return failing
  }

  /// An agent's name by its id in `atlas agents` results.
  ///
  /// - Parameter id: The id, such as `claude` or `claude-app`.
  /// - Returns: "Claude Code", "Codex", "Claude app", or the id itself for one this app does not know.
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
