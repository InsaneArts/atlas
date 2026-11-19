/// The choice of where agents work: Second Login, a VM and Cloud, in that order.
public struct OnboardingDesktopScreenState: Equatable, Sendable {
  /// The rows, in order.
  public var items: [OnboardingDesktopItem]

  /// Creates the choice's state.
  ///
  /// - Parameter items: The rows, in order.
  public init(items: [OnboardingDesktopItem]) {
    self.items = items
  }
}
