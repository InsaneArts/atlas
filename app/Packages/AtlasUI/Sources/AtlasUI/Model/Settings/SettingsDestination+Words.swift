extension SettingsDestination {
  /// The authoritative page title, also used by the native window.
  public var title: String {
    switch self {
    case .root(.helper): "Helper"
    case .root(.general): "General"
    case .root(.screen): "Screen & Viewer"
    case .root(.notifications): "Notifications"
    case .root(.agents): "Agents"
    case .root(.about): "About"
    case .diagnostics: "Readiness & Diagnostics"
    case .projects: "Projects"
    }
  }

  /// The page's scope, shown above its sections.
  public var description: String {
    switch self {
    case .root(.helper): "The hidden login where agents use your apps."
    case .root(.general): "Choose how Atlas starts and find everyday preferences."
    case .root(.screen): "Choose how often Atlas's windows show the helper's screen."
    case .root(.notifications): "Choose whether Atlas notifies you when it needs you."
    case .root(.agents): "Connect your installed agents to Atlas."
    case .root(.about): "Atlas's version, updates and setup."
    case .diagnostics: "See what is ready and what needs your attention."
    case .projects: "Manage the Atlas instructions in projects you have taught."
    }
  }

  /// Every persistent scrolling page, in sidebar order with its details.
  public static let pages: [Self] = [
    .root(.helper), .diagnostics, .root(.general), .root(.screen), .root(.notifications),
    .root(.agents), .projects, .root(.about),
  ]
}
