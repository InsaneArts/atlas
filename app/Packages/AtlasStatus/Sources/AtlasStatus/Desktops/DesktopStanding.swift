import AtlasService

/// How a desktop stands, as Settings' list says it after what the desktop is, and its dot: a VM by its power, or in
/// maintenance while the person uses it; any desktop as needing setup while agents can't use it though it runs. The
/// app words each and colours its dot.
public enum DesktopStanding: Equatable, Sendable {
  /// Agents can use it.
  case ready
  /// A VM that runs.
  case running
  /// A VM that is starting.
  case starting
  /// A VM that is stopping.
  case stopping
  /// A VM that is stopped: it starts by itself when an agent needs it.
  case stopped
  /// A VM the person has open for maintenance: agents wait until they end it.
  case maintenance
  /// Agents can't use it yet, though it is there to use.
  case needsSetup
  /// Nothing was read of it yet.
  case unknown

  /// How `entry` stands. Second Login by what agents can do there. A VM in maintenance while its maintenance is on,
  /// or ending; otherwise by its power, a running one needing setup while agents can't use it; unknown while its power
  /// is not said.
  ///
  /// - Parameter entry: A desktop the service serves.
  /// - Returns: How it stands.
  public static func of(_ entry: DesktopList.Entry) -> DesktopStanding {
    guard entry.kind == .vm else { return of(entry.standing) }
    let facts = entry.facts ?? DesktopList.Facts()
    if facts.maintenance == "on" || facts.maintenance == "ending" { return .maintenance }
    switch VMPower(word: facts.power) {
    case .running: return of(entry.standing) == .ready ? .running : .needsSetup
    case .starting: return .starting
    case .stopping: return .stopping
    case .stopped: return .stopped
    case nil: return .unknown
    }
  }

  /// How a desktop stands by what agents can do there: ready unless it is not ready or needs the person.
  ///
  /// - Parameter state: What it is doing, as its newest status read said; nil while none was read.
  /// - Returns: Ready, needs setup, or unknown.
  static func of(_ state: OverviewState?) -> DesktopStanding {
    switch state {
    case nil: .unknown
    case .notReady, .needsYou, .unknown: .needsSetup
    case .working, .idle, .control, .stopped: .ready
    }
  }

  /// Whether it keeps Settings' Desktops row from saying "Ready": it needs setup.
  public var needsSetup: Bool { self == .needsSetup }
}
