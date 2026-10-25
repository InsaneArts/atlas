/// The last screen in the person's account.
public struct OnboardingDoneScreenState: Equatable, Sendable {
  /// What to do after a restart, naming the helper.
  public var lede: String
  /// Whether Open Atlas at login is on.
  public var opensAtLogin: Bool
  /// Whether the Open Atlas at login switch takes changes.
  public var isEnabled: Bool

  /// Creates the done screen's state.
  ///
  /// - Parameters:
  ///   - lede: What to do after a restart, naming the helper.
  ///   - opensAtLogin: Whether Open Atlas at login is on.
  ///   - isEnabled: Whether the Open Atlas at login switch takes changes.
  public init(lede: String, opensAtLogin: Bool, isEnabled: Bool = true) {
    self.lede = lede
    self.opensAtLogin = opensAtLogin
    self.isEnabled = isEnabled
  }
}
