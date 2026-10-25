extension SettingsAgentsState {
  /// The actual loaded project count, including an honest empty state.
  public var projectsSummary: String {
    switch projects.count {
    case 0: "No projects taught"
    case 1: "1 project taught"
    default: "\(projects.count) projects taught"
    }
  }
}
