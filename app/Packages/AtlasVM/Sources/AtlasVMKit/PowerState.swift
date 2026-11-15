/// Whether a VM runs, as its runner last saw.
public enum PowerState: String, Codable, Equatable, Sendable {
  /// It was never made, or was removed.
  case absent
  /// It is made and not running.
  case stopped
  /// It is starting.
  case starting
  /// It runs.
  case running
  /// It is stopping.
  case stopping
  /// Its runner cannot tell.
  case unknown
}
