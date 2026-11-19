/// Whether the add of the VM a first setup makes still goes on.
public enum VMAdd: Equatable, Sendable {
  /// It runs, here or in a process the record names.
  case running
  /// Its process ended before the VM was set up, with no reason: Go On asks it again.
  case stopped
  /// It stopped short for a reason: Try Again asks it again.
  ///
  /// - Parameter reason: Why, in its own words.
  case failed(String)

  /// How the add of `vm` stands: running while this window runs it or its process runs; else failed with this
  /// window's last reason, then the record's; else stopped.
  ///
  /// - Parameters:
  ///   - vm: The VM, once its add has a record.
  ///   - runningHere: Whether this window runs the add now.
  ///   - failedHere: Why the add this window ran last failed; nil when it did not, or ran none.
  /// - Returns: How it stands.
  public static func of(_ vm: VMFacts?, runningHere: Bool, failedHere: String?) -> VMAdd {
    if runningHere { return .running }
    if vm?.adding == true, vm?.adderRunning != false { return .running }
    if let reason = failedHere ?? vm?.failure { return .failed(reason) }
    return .stopped
  }
}
