/// The agents screen: which agents to connect.
public struct OnboardingAgentsScreenState: Equatable, Sendable {
  /// The agents, in order.
  public var agents: [OnboardingAgentItem]

  /// Creates the agents screen's state.
  ///
  /// - Parameter agents: The agents, in order.
  public init(agents: [OnboardingAgentItem]) {
    self.agents = agents
  }
}
