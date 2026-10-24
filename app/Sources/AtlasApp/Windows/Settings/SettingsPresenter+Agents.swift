import AtlasCommands
import AtlasUI
import Foundation

extension SettingsPresenter {
  /// The Agents pane: Claude Code, Codex and the Claude app, each connected, ready to connect, or saying why not;
  /// then each project taught.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The pane.
  static func agents(_ snapshot: SettingsSnapshot) -> SettingsAgentsState {
    let agents = snapshot.agents
    let installing = snapshot.facts.installing
    let plugins = agents.choices.map { choice in
      SettingsAgentRow(
        id: choice.id.rawValue,
        title: Words.agentTitle(choice.id),
        detail: SettingsWords.usesAtlas,
        icon: icon(choice.id.rawValue, snapshot.icons),
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
      detail: SettingsWords.showsHelper,
      icon: icon(AgentsStatus.claudeAppID, snapshot.icons),
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
      agents: plugins + [app],
      projects: snapshot.agents.projects.map(project),
      notice: notice(snapshot, in: .agents)
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
    if !found { return .note(SettingsWords.notInstalled) }
    return .connect(isEnabled: installing.isEmpty)
  }

  /// An agent's name by its id in `atlas agents` results.
  ///
  /// - Parameter id: The id, such as `claude` or `claude-app`.
  /// - Returns: "Claude Code", "Codex", "Claude app", or the id itself for one this app does not know.
  static func agentTitle(_ id: String) -> String {
    if let agent = AgentID(rawValue: id) { return Words.agentTitle(agent) }
    return id == AgentsStatus.claudeAppID ? SettingsWords.claudeApp : id
  }

  /// An agent's icon: its app's own where the app is on this Mac, else a tile of its own, Claude Code's unlike Codex's.
  ///
  /// - Parameters:
  ///   - id: The agent's id.
  ///   - icons: The icons of the agents whose apps are on this Mac.
  /// - Returns: The icon.
  static func icon(_ id: String, _ icons: [String: IconImage]) -> RowIcon {
    if let icon = icons[id] { return .app(icon) }
    return switch id {
    case AgentID.claude.rawValue: .tile(.symbol("asterisk"))
    case AgentsStatus.claudeAppID: .tile(.symbol("bubble.left"))
    default: .tile(.text(">_"))
    }
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
