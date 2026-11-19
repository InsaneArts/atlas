/// One row of the choice of where agents work: the kind of desktop, and whether it can be picked now.
public struct OnboardingDesktopItem: Equatable, Sendable, Identifiable {
  /// The kind of desktop the row picks, which names it.
  public var id: OnboardingDesktopKind
  /// Whether it can be picked now, and whether it is.
  public var standing: OnboardingDesktopStanding

  /// Creates one row of the choice.
  ///
  /// - Parameters:
  ///   - id: The kind of desktop the row picks.
  ///   - standing: Whether it can be picked now, and whether it is.
  public init(id: OnboardingDesktopKind, standing: OnboardingDesktopStanding) {
    self.id = id
    self.standing = standing
  }
}
