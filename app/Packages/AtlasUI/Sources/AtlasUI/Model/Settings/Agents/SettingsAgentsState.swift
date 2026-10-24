/// The Agents pane: the agents connected to Atlas, and the projects it taught.
public struct SettingsAgentsState: Equatable, Sendable {
  /// The agents, in order.
  public var agents: [SettingsAgentRow]
  /// The projects taught, in order; none shows a line saying so.
  public var projects: [SettingsProjectRow]
  /// What the last Connect or Forget said, under the agents; nil for nothing.
  public var notice: SettingsNote?

  /// Creates the pane's state.
  ///
  /// - Parameters:
  ///   - agents: The agents, in order.
  ///   - projects: The projects taught, in order.
  ///   - notice: What the last Connect or Forget said.
  public init(agents: [SettingsAgentRow], projects: [SettingsProjectRow], notice: SettingsNote? = nil) {
    self.agents = agents
    self.projects = projects
    self.notice = notice
  }
}
