/// The Agents pane: the agents connected to Atlas, and the projects it taught.
public struct SettingsAgentsState: Equatable, Sendable {
  /// The agents, in order.
  public var agents: [SettingsAgentRow]
  /// The projects last read successfully, in order.
  public var projects: [SettingsProjectRow]
  /// Whether the latest project reading is pending, loaded or failed.
  public var projectsReading: SettingsProjectsReading
  /// What the last Connect or Forget said, under the agents; nil for nothing.
  public var notice: SettingsNote?

  /// Creates the pane's state.
  ///
  /// - Parameters:
  ///   - agents: The agents, in order.
  ///   - projects: The projects taught, in order.
  ///   - projectsReading: The latest reading's state; loaded for supplied rows by default.
  ///   - notice: What the last Connect or Forget said.
  public init(
    agents: [SettingsAgentRow],
    projects: [SettingsProjectRow],
    projectsReading: SettingsProjectsReading = .loaded,
    notice: SettingsNote? = nil
  ) {
    self.agents = agents
    self.projects = projects
    self.projectsReading = projectsReading
    self.notice = notice
  }
}
