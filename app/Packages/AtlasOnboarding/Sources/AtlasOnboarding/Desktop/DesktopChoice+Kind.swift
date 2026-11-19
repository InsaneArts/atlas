extension DesktopChoice {
  /// Where agents work, in the order the choice lists them.
  public enum Kind: CaseIterable, Equatable, Hashable, Sendable {
    /// A hidden second login on this Mac.
    case secondLogin
    /// A virtual Mac on this Mac.
    case vm
    /// A Mac in the cloud.
    case cloud
  }
}
