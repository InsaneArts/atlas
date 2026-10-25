/// A stable, non-executable location in Settings.
public enum SettingsSearchTarget: Hashable, Sendable {
  /// A page heading.
  case page(SettingsDestination)
  /// The root page's link to one of its details.
  case disclosure(SettingsDestination)
  /// The startup preference.
  case startup
  /// The screen cadence.
  case cadence
  /// The notification preference.
  case notifications
  /// The helper account.
  case account
  /// A stale login's cleanup row.
  case cleanup
  /// The helper readiness section.
  case readiness
  /// One reported readiness check.
  case check(String)
  /// The recovery row.
  case recover
  /// The logout row.
  case logOut
  /// The diagnostics support row.
  case support
  /// One agent row.
  case agent(String)
  /// One already loaded project row.
  case project(String)
  /// Atlas's identity and version.
  case identity
  /// The update controls.
  case updates
  /// The setup row.
  case setup

  /// The page that owns this target.
  public var destination: SettingsDestination {
    switch self {
    case .page(let destination): destination
    case .disclosure(let destination): .root(destination.root)
    case .startup: .root(.general)
    case .cadence: .root(.screen)
    case .notifications: .root(.notifications)
    case .account, .cleanup, .recover, .logOut: .root(.helper)
    case .readiness, .check, .support: .diagnostics
    case .agent: .root(.agents)
    case .project: .projects
    case .identity, .updates, .setup: .root(.about)
    }
  }
}
