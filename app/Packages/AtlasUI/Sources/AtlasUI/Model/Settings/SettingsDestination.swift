/// A stable Settings location, independent of its displayed title or live status.
public enum SettingsDestination: Hashable, Sendable {
  /// A root page.
  case root(SettingsRoot)
  /// All helper readiness checks and diagnostics.
  case diagnostics
  /// The projects taught to agents.
  case projects

  /// The sidebar selection and saved preference that own this location.
  public var root: SettingsRoot {
    switch self {
    case .root(let root): root
    case .diagnostics: .helper
    case .projects: .agents
    }
  }
}
