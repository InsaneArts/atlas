/// The last screen in the person's account.
public struct OnboardingDoneScreenState: Equatable, Sendable {
  /// What to do after a restart, naming the helper.
  public var lede: String
  /// Whether Open Atlas at login is on.
  public var opensAtLogin: Bool
  /// Whether the box to keep the home folder private from the helper is checked.
  public var keepsHomePrivate: Bool
  /// Whether the box shows: only with a helper, which a VM is not.
  public var offersHomePrivacy: Bool
  /// Whether Open Atlas at login shows: on a first setup, never once Atlas is set up and a desktop was added beside.
  public var offersOpenAtLogin: Bool
  /// Whether the screen's switches take changes.
  public var isEnabled: Bool

  /// Creates the done screen's state.
  ///
  /// - Parameters:
  ///   - lede: What to do after a restart, naming the helper.
  ///   - opensAtLogin: Whether Open Atlas at login is on.
  ///   - keepsHomePrivate: Whether the box to keep the home folder private is checked; checked by default.
  ///   - offersHomePrivacy: Whether the box shows; true by default, false for a VM.
  ///   - offersOpenAtLogin: Whether Open Atlas at login shows; true by default, false for a desktop added beside.
  ///   - isEnabled: Whether the screen's switches take changes.
  public init(
    lede: String,
    opensAtLogin: Bool,
    keepsHomePrivate: Bool = true,
    offersHomePrivacy: Bool = true,
    offersOpenAtLogin: Bool = true,
    isEnabled: Bool = true
  ) {
    self.lede = lede
    self.opensAtLogin = opensAtLogin
    self.keepsHomePrivate = keepsHomePrivate
    self.offersHomePrivacy = offersHomePrivacy
    self.offersOpenAtLogin = offersOpenAtLogin
    self.isEnabled = isEnabled
  }
}
