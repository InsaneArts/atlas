/// One row of a VM's page in Settings, with what its button can do now; the app words it and draws it.
public enum VMPageRow: Equatable, Sendable {
  /// Its power, with Stop… while it runs or starts, or Start while it is stopped: each `enabled` only while the
  /// service says it can; no button while it stops.
  case power(VMPower, button: PowerButton?)
  /// Open for maintenance, with Open… or, while `isOn`, End…; `enabled` while the service says either can.
  case maintenance(isOn: Bool, enabled: Bool)
  /// How much of its disk is used, of the most it may grow to; `used` nil while not said.
  case disk(used: Int64?, size: Int64)
  /// Its macOS version, and a newer one this Mac can install, when there is one.
  case macOS(version: String, newer: String?)
  /// How its Atlas stands to this Mac's, and what brings it there when it is not this Mac's.
  case atlas(AtlasState, next: String?)
  /// Remove this VM, which the approval window confirms.
  case remove

  /// A power row's button.
  public enum PowerButton: Equatable, Sendable {
    /// Stop…, asking first.
    case stop(enabled: Bool)
    /// Start.
    case start(enabled: Bool)
  }

  /// How a VM's Atlas stands to this Mac's.
  public enum AtlasState: Equatable, Sendable {
    /// The very build this Mac runs.
    case current
    /// An older one, which an update brings to this Mac's.
    case available
    /// A newer one than this Mac's.
    case newer
    /// An update is bringing it to this Mac's.
    case updating
    /// Not known.
    case unknown

    /// The state the service's word `word` says; unknown for any other.
    ///
    /// - Parameter word: The service's word, such as `current`.
    init(word: String?) {
      switch word {
      case "current": self = .current
      case "available": self = .available
      case "newer": self = .newer
      case "updating": self = .updating
      default: self = .unknown
      }
    }
  }

  /// The group of the page the row stands in.
  public var section: VMPageSection {
    switch self {
    case .power, .maintenance: .use
    case .disk, .macOS, .atlas: .about
    case .remove: .removal
    }
  }
}
