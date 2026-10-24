import AtlasCommands
import AtlasOnboarding

/// The agents step: which agents are installed and connected, their switches, and connecting the chosen ones.
extension OnboardingController {
  /// Lists the agents Atlas can connect, then queues asking `atlas agents status` which are installed and connected;
  /// never a second time while it waits or runs, and not once the window has closed.
  func loadAgents() {
    guard !closed, !isQueued(.readAgents) else { return }
    if snapshot.agents.isEmpty { snapshot.agents = AgentChoice.all }
    readingsSinceAgentsFailed = 0
    enqueue(.readAgents)
  }

  /// Asks `atlas agents status`. A reading that fails keeps the agents listed, unchecked with their switches off, says
  /// so and lets Continue skip the step; it is asked again every ``agentsRetryReadings`` readings. One that ends after
  /// the window closed is dropped.
  func readAgents() async {
    let outcome = await commands.run(.agentsStatus)
    guard !closed else { return }
    guard outcome.ok, let status = outcome.decode(AgentsStatus.self) else {
      snapshot.agentsFailed = true
      if snapshot.working == nil { snapshot.problem = OnboardingWords.agentsCheckFailed }
      return
    }
    if snapshot.agentsFailed, snapshot.problem == OnboardingWords.agentsCheckFailed { snapshot.problem = nil }
    snapshot.agentsFailed = false
    snapshot.agents = AgentChoice.merged(snapshot.agents, with: status.agents)
    snapshot.agentsLoaded = true
  }

  /// Counts a reading since `atlas agents status` failed, and asks it again once enough have passed.
  func retryAgents() {
    guard !isQueued(.readAgents) else { return }
    readingsSinceAgentsFailed += 1
    if readingsSinceAgentsFailed >= Self.agentsRetryReadings { loadAgents() }
  }

  /// Turns an agent's switch on or off.
  ///
  /// - Parameters:
  ///   - id: The agent's id, as the screen names it.
  ///   - on: Whether it is on.
  func setAgent(_ id: String, on: Bool) {
    guard let index = snapshot.agents.firstIndex(where: { $0.id.rawValue == id }) else { return }
    snapshot.agents[index].on = on
  }

  /// The agents switched on now, in the screen's order: what Continue chooses as it is pressed.
  var agentsSwitchedOn: [AgentID] {
    snapshot.agents
      .filter(\.on)
      .map(\.id)
  }

  /// Connects the agents chosen as Continue was pressed that still need connecting now. Agents that cannot be
  /// connected are each named with why, and the window stays.
  ///
  /// - Parameter switchedOn: The agents switched on when Continue was pressed (``agentsSwitchedOn``): one a reading
  ///   switched on since is left out.
  func connectAgents(_ switchedOn: [AgentID]) async {
    let chosen = AgentChoice.toConnect(snapshot.agents)
      .filter(switchedOn.contains)
    if !chosen.isEmpty {
      let installs = dependencies.installs
      guard let outcome = await act(OnboardingWords.connecting, { await installs.install(chosen) }) else { return }
      let change = outcome.decode(AgentsChange.self)
      // A reply that names no agent connected them all when it worked.
      let connected = outcome.ok ? chosen : change?.connected ?? []
      for index in snapshot.agents.indices where connected.contains(snapshot.agents[index].id) {
        snapshot.agents[index].connected = true
      }
      guard outcome.ok else {
        let failures = (change?.failures ?? [])
          .map { OnboardingWords.notConnected($0.agent, $0.error) }
        if !failures.isEmpty { snapshot.problem = failures.joined(separator: " ") }
        return
      }
    }
    // A note left here (a skipped check, or an install that failed before its agent was switched off) belongs to the
    // agents step only.
    snapshot.problem = nil
    snapshot.choices.agentsDone = true
  }
}
