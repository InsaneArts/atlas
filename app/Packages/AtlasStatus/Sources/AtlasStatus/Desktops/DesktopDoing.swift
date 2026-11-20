import AtlasService

/// What a desktop the app does not view is doing, as its menu says it beside its title and the menu bar's dot counts
/// it: what agents do there, as its newest status read said, or, for a VM that does not run for agents now, its power
/// or the person's maintenance. The desktop viewed is said by the window's own mode, which knows more.
public enum DesktopDoing: Equatable, Sendable {
  /// What agents do there, as the status table's row says it.
  case mode(Mode.Kind)
  /// A VM that is starting, stopping or stopped, or that the person has open for maintenance: agents wait on it, and
  /// nothing there asks anything of the person.
  case standing(DesktopStanding)

  /// What `entry` is doing. A VM is said by its standing while it starts, stops, is stopped or in maintenance, and
  /// connecting while its power is not said; any other desktop by its newest status read, connecting while none was
  /// read. Control there is never this app's, which controls only the desktop it views, so it reads as in use
  /// elsewhere.
  ///
  /// - Parameter entry: A desktop the service serves.
  /// - Returns: What it is doing.
  public static func of(_ entry: DesktopList.Entry) -> DesktopDoing {
    if entry.kind == .vm {
      let standing = DesktopStanding.of(entry)
      switch standing {
      case .maintenance, .starting, .stopping, .stopped: return .standing(standing)
      case .unknown: return .mode(.connecting)
      case .ready, .running, .needsSetup: break
      }
    }
    guard let state = entry.standing else { return .mode(.connecting) }
    return switch state {
    case .working: .mode(.working)
    case .idle: .mode(.idle)
    case .control: .mode(.inUseElsewhere)
    case .needsYou: .mode(.needsYou)
    case .stopped: .mode(.stopped)
    case .notReady, .unknown: .mode(.notReady)
    }
  }

  /// The row the menu bar's dot would show for it: its mode, or nothing while a VM only waits on its power or the
  /// person's maintenance.
  public var kind: Mode.Kind? {
    switch self {
    case .mode(let kind): kind
    case .standing: nil
    }
  }
}
