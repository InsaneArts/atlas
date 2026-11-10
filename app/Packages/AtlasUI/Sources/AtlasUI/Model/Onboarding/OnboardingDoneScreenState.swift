/// The last screen in the person's account.
public struct OnboardingDoneScreenState: Equatable, Sendable {
  /// What to do after a restart, naming the helper.
  public var lede: String
  /// Whether Open Atlas at login is on.
  public var opensAtLogin: Bool
  /// Whether the box to keep the home folder private from the helper is checked.
  public var keepsHomePrivate: Bool
  /// Whether the screen's switches take changes.
  public var isEnabled: Bool

  /// Creates the done screen's state.
  ///
  /// - Parameters:
  ///   - lede: What to do after a restart, naming the helper.
  ///   - opensAtLogin: Whether Open Atlas at login is on.
  ///   - keepsHomePrivate: Whether the box to keep the home folder private is checked; checked by default.
  ///   - isEnabled: Whether the screen's switches take changes.
  public init(lede: String, opensAtLogin: Bool, keepsHomePrivate: Bool = true, isEnabled: Bool = true) {
    self.lede = lede
    self.opensAtLogin = opensAtLogin
    self.keepsHomePrivate = keepsHomePrivate
    self.isEnabled = isEnabled
  }
}
