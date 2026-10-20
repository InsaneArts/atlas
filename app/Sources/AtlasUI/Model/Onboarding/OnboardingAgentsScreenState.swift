/// The agents screen: which agents to connect, and which projects to teach.
package struct OnboardingAgentsScreenState: Equatable, Sendable {
  /// The agents, in order.
  package var agents: [OnboardingAgentItem]
  /// The chosen project folders, in the order chosen.
  package var projects: [OnboardingProjectItem]

  /// Creates the agents screen's state.
  ///
  /// - Parameters:
  ///   - agents: The agents, in order.
  ///   - projects: The chosen project folders, in the order chosen.
  package init(agents: [OnboardingAgentItem], projects: [OnboardingProjectItem]) {
    self.agents = agents
    self.projects = projects
  }
}
