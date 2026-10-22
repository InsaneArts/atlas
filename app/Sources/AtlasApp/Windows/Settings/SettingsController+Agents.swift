import AtlasKit

/// The Agents pane's work: reading which agents are connected and which projects are taught, connecting one agent,
/// and forgetting a project.
extension SettingsController {
  /// Reads the agents afresh, replacing any reading still under way.
  func readAgents() {
    agentsReading?.cancel()
    agentsReading = Task { await loadAgents() }
  }

  /// Reads `atlas agents status`. Only the newest reading started is kept; one that fails says so on each agent that
  /// was never reported, and keeps the projects last read.
  func loadAgents() async {
    agentsGeneration += 1
    let number = agentsGeneration
    let outcome = await dependencies.commands.run(.agentsStatus)
    guard number == agentsGeneration, !Task.isCancelled else { return }
    guard outcome.ok, let status = outcome.decode(AgentsStatus.self) else {
      agents.failed = true
      return
    }
    agents = SettingsAgents(
      choices: AgentChoice.merged(AgentChoice.all, with: status.agents),
      app: status.claudeApp,
      projects: status.projects ?? [],
      failed: false
    )
  }

  /// Connects one agent alone: Claude Code or Codex by name, or the Claude app, through the installs every window
  /// shares, so one runs at a time app-wide; the agents are read again after.
  ///
  /// - Parameter id: The agent's id, as its row names it.
  func connect(_ id: String) async {
    let installs = dependencies.installs
    guard !installs.isBusy else { return say(HelperWords.waitForAction, in: .agents, problem: true) }
    say(nil, in: .agents)
    let outcome: Outcome?
    if let agent = AgentID(rawValue: id) {
      outcome = await installs.install([agent])
    } else if id == AgentsStatus.claudeAppID {
      outcome = await installs.connectClaudeApp()
    } else {
      return
    }
    guard let outcome else { return say(HelperWords.waitForAction, in: .agents, problem: true) }
    if !outcome.ok { say(Self.connectFailure(outcome), in: .agents, problem: true) }
    await loadAgents()
  }

  /// Why a Connect did not work: each agent that failed, with why, else the command's own message.
  ///
  /// - Parameter outcome: The failed command's outcome.
  /// - Returns: The line.
  static func connectFailure(_ outcome: Outcome) -> String {
    let failed = (outcome.decode(AgentsChange.self)?.agents ?? [])
      .filter { $0.ok != true }
      .map { SettingsWords.notConnected(SettingsPresenter.agentTitle($0.agent), $0.error ?? "") }
    return failed.isEmpty ? Words.sentence(outcome.message) : failed.joined(separator: " ")
  }

  /// Asks before forgetting a project taught.
  ///
  /// - Parameter path: The project's folder.
  func askForget(_ path: String) {
    guard agents.projects.contains(where: { $0.path == path }) else { return }
    ask(.forget(path))
  }

  /// Forgets a project once the person confirmed: `atlas agents forget` takes the Atlas section out of its files and
  /// off the list. One that fails says why and stays listed; the projects are read again either way.
  ///
  /// - Parameter path: The project's folder.
  func forget(_ path: String) async {
    let outcome = await dependencies.commands.run(.forgetProject(path: path))
    session.alert = nil
    let name = SettingsPresenter.projectName(path)
    say(outcome.ok ? nil : SettingsWords.notForgotten(name, outcome.message), in: .agents, problem: true)
    await loadAgents()
  }
}
