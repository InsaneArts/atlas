/// A persistent root destination in Settings.
public enum SettingsRoot: String, CaseIterable, Hashable, Sendable {
  /// Startup and everyday preferences.
  case general
  /// The hidden login and its readiness.
  case helper
  /// Screen update preferences.
  case screen
  /// Notification preferences.
  case notifications
  /// Connected agents and taught projects.
  case agents
  /// Version, updates and setup.
  case about

  /// Restores the shipped pane values, falling back to General for a missing or invalid value.
  ///
  /// - Parameter savedValue: The value stored under the existing Settings selection key.
  public init(savedValue: String?) {
    self = savedValue.flatMap(Self.init(rawValue:)) ?? .general
  }

  /// Creates the root that owns an existing content pane.
  ///
  /// - Parameter pane: The existing pane.
  public init(pane: SettingsTab) {
    switch pane {
    case .general: self = .general
    case .helper: self = .helper
    case .agents: self = .agents
    case .about: self = .about
    }
  }

  /// The original content pane, or nil for a root introduced by the page split.
  public var pane: SettingsTab? {
    switch self {
    case .general: .general
    case .helper: .helper
    case .agents: .agents
    case .about: .about
    case .screen, .notifications: nil
    }
  }
}
