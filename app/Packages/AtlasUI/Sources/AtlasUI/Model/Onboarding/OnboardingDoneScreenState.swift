/// The last screen in the person's account.
public struct OnboardingDoneScreenState: Equatable, Sendable {
  /// What to do after a restart, naming the helper.
  public var lede: String
  /// Whether Open Atlas at login is on.
  public var opensAtLogin: Bool

  /// Creates the done screen's state.
  ///
  /// - Parameters:
  ///   - lede: What to do after a restart, naming the helper.
  ///   - opensAtLogin: Whether Open Atlas at login is on.
  public init(lede: String, opensAtLogin: Bool) {
    self.lede = lede
    self.opensAtLogin = opensAtLogin
  }
}
