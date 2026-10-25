/// One of the two permissions Atlas needs in the helper, and where it stands.
public struct OnboardingPermissionItem: Equatable, Sendable, Identifiable {
  /// The permission's id: what opening its settings sends.
  public var id: String
  /// Its name, as System Settings shows it.
  public var title: String
  /// Whether it is on.
  public var isOn: Bool
  /// Where it stands, in words.
  public var standing: OnboardingPermissionStanding
  /// Whether it is the one allowed now: on the screen, the next to allow; in the panel, the one the panel guides.
  public var isCurrent: Bool
  /// Whether it was read: Atlas's job did not answer while it restarts, so it is being checked, never off.
  public var isKnown: Bool

  /// Creates one permission as the helper shows it.
  ///
  /// - Parameters:
  ///   - id: The permission's id: what opening its settings sends.
  ///   - title: Its name, as System Settings shows it.
  ///   - isOn: Whether it is on.
  ///   - standing: Where it stands, in words.
  ///   - isCurrent: Whether it is the one allowed now; false by default.
  ///   - isKnown: Whether it was read; true by default.
  public init(
    id: String,
    title: String,
    isOn: Bool,
    standing: OnboardingPermissionStanding,
    isCurrent: Bool = false,
    isKnown: Bool = true
  ) {
    self.id = id
    self.title = title
    self.isOn = isOn
    self.standing = standing
    self.isCurrent = isCurrent
    self.isKnown = isKnown
  }
}
