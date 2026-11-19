extension VMFacts {
  /// Which of the VM path's screens its add has reached.
  public enum Stage: Equatable, Sendable {
    /// macOS comes down from Apple, or the add has not started making the VM yet.
    case download
    /// Atlas makes the VM: macOS installed, the VM started, Atlas set up inside and connected.
    case install
    /// The person allows Atlas in the VM's window.
    case allow
    /// The add is set up or done: Atlas checks the VM works.
    case check
  }

  /// The screen the add has reached, by its record's steps and Atlas VM's word on its making.
  public var stage: Stage {
    if !adding || setup == .done || steps.contains(.ready) { return .check }
    if steps.contains(.shown) { return .allow }
    if steps.contains(.created) { return .install }
    switch progress?.phase {
    case .installing, .done: return .install
    case .checking, .downloading, .unknown, nil: return .download
    }
  }
}
