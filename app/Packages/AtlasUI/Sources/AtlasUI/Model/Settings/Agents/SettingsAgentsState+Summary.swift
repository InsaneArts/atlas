extension SettingsAgentsState {
  /// The loaded count, or an honest indication that the latest reading is unavailable.
  public var projectsSummary: String {
    if projectsReading == .reading { return "Reading projects…" }
    if projectsReading == .failed {
      return projects.isEmpty ? "Couldn't read projects" : "\(projectsCount) · Couldn't refresh projects"
    }
    return projectsCount
  }

  /// Progress or failure shown beside any cached rows.
  public var projectsNote: SettingsNote? {
    switch projectsReading {
    case .reading: SettingsNote(text: "Reading projects…", kind: .working)
    case .loaded: nil
    case .failed:
      SettingsNote(
        text: projects.isEmpty
          ? "Couldn't read projects. Try again." : "Couldn't refresh projects. Showing the last reading.",
        kind: .problem
      )
    }
  }

  /// The number of projects in the last successful reading.
  private var projectsCount: String {
    switch projects.count {
    case 0: "No projects taught"
    case 1: "1 project taught"
    default: "\(projects.count) projects taught"
    }
  }
}
