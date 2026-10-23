/// The agents screen: which agents to connect.
package struct OnboardingAgentsScreenState: Equatable, Sendable {
  /// The agents, in order.
  package var agents: [OnboardingAgentItem]

  /// Creates the agents screen's state.
  ///
  /// - Parameter agents: The agents, in order.
  package init(agents: [OnboardingAgentItem]) {
    self.agents = agents
  }
}
