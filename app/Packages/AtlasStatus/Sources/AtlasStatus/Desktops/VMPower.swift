/// A VM's power, as its keeper last saw Atlas VM say it.
public enum VMPower: Equatable, Sendable {
  /// It runs.
  case running
  /// It is starting.
  case starting
  /// It is stopping.
  case stopping
  /// It is stopped, or not made yet.
  case stopped

  /// The power the service's word `word` says; nil for none, or a word that says nothing sure.
  ///
  /// - Parameter word: The service's word, such as `running`.
  public init?(word: String?) {
    switch word {
    case "running": self = .running
    case "starting": self = .starting
    case "stopping": self = .stopping
    case "stopped", "absent": self = .stopped
    default: return nil
    }
  }
}
