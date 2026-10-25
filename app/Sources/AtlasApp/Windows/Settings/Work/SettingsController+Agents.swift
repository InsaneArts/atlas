import AtlasCommands
import AtlasOnboarding
import AtlasService
import Foundation

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
  ///
  /// - Parameter preservingFailures: Failures from the command that requested this reading, kept until a later reading.
  func loadAgents(preservingFailures: Set<String> = []) async {
    agentsGeneration += 1
    let number = agentsGeneration
    agents.projectsReading = .reading
    let outcome = await dependencies.commands.run(.agentsStatus)
    guard number == agentsGeneration, !Task.isCancelled else { return }
    guard outcome.ok, let status = outcome.decode(AgentsStatus.self) else {
      agents.failed = true
      agents.projectsReading = .failed
      return
    }
    agents = SettingsAgents(
      choices: AgentChoice.merged(AgentChoice.all, with: status.agents),
      projects: status.projects ?? [],
      projectsReading: .loaded,
      failed: false
    )
    for choice in agents.choices where choice.checked && choice.problem == nil {
      let id = choice.id.rawValue
      guard !preservingFailures.contains(id), let failure = session.connectFailures[id] else { continue }
      if failure.isDisconnect ? choice.disconnected : choice.connected {
        session.connectFailures[id] = nil
      }
    }
    session.search.selection = state.search.selection
  }

  /// Connects every installed part of one agent through the installs every window
  /// shares, so one runs at a time app-wide; the agents are read again after. Why it failed shows in the agent's own
  /// row, until the next Connect or a later reading confirms it is connected.
  ///
  /// - Parameter id: The agent's id, as its row names it.
  func connect(_ id: String) async {
    let installs = dependencies.installs
    guard !installs.isBusy else { return say(HelperWords.waitForAction, in: .agents, kind: .working) }
    say(nil, in: .agents)
    session.connectFailures[id] = nil
    let outcome: Outcome?
    if let agent = AgentID(rawValue: id) {
      outcome = await installs.install([agent], claudeParts: agents.choices.first { $0.id == .claude }?.parts ?? [])
    } else {
      return
    }
    guard let outcome else { return say(HelperWords.waitForAction, in: .agents, kind: .working) }
    var failedAgents: Set<String> = []
    if !outcome.ok {
      let failures = Self.connectFailures(outcome, asked: id)
        .mapValues { SettingsAgentFailure(message: $0) }
      session.connectFailures.merge(failures) { _, newer in newer }
      failedAgents = Set(failures.keys)
    }
    await loadAgents(preservingFailures: failedAgents)
  }

  /// Disconnects all the installed parts in an agent's row, then reads their status again.
  ///
  /// - Parameter id: The displayed agent id.
  func disconnect(_ id: String) async {
    guard let choice = agents.choices.first(where: { $0.id.rawValue == id }), !choice.parts.isEmpty else { return }
    session.connectFailures[id] = nil
    guard let outcome = await dependencies.installs.disconnect(id, parts: choice.parts) else { return }
    if !outcome.ok {
      session.connectFailures[id] = SettingsAgentFailure(
        message: "Couldn't disconnect: " + (Self.connectFailures(outcome, asked: id)[id] ?? outcome.message),
        isDisconnect: true
      )
    }
    await loadAgents()
  }

  /// Why a Connect did not work, by agent: each agent the command says failed, with why; else the agent asked for,
  /// with the command's own message.
  ///
  /// - Parameters:
  ///   - outcome: The failed command's outcome.
  ///   - id: The agent asked for.
  /// - Returns: Why, by the agent's id.
  static func connectFailures(_ outcome: Outcome, asked id: String) -> [String: String] {
    let failed = (outcome.decode(AgentsChange.self)?.agents ?? [])
      .filter { $0.ok != true }
      .map { row in
        let key = row.agent == AgentsStatus.claudeAppID ? AgentID.claude.rawValue : row.agent
        return (key, "\(AgentsStatus.partTitle(row.agent)): \(row.error ?? "")")
      }
    guard !failed.isEmpty else { return [id: outcome.message] }
    return Dictionary(failed, uniquingKeysWith: { first, next in "\(first); \(next)" })
  }

  /// Shows the file an agent's failed Connect is in, in the Finder: the Claude app's settings file.
  ///
  /// - Parameter id: The agent's id.
  func showAgentFile(_ id: String) {
    guard let file = AgentFiles.settings(of: id, home: FileManager.default.homeDirectoryForCurrentUser) else { return }
    dependencies.revealFile(file.path)
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
    let name = SettingsPresenter.projectName(path)
    say(outcome.ok ? nil : SettingsWords.notForgotten(name, outcome.message), in: .agents, kind: .problem)
    await loadAgents()
    session.alert = nil
  }
}
