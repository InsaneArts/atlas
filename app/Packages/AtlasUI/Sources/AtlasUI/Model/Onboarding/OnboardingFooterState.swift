/// The bar under a setup screen. Its defaults show no buttons and no dots, as the permission panel and the first reading have.
public struct OnboardingFooterState: Equatable, Sendable {
  /// Where the screen sits among the dots in the middle of the bar.
  public struct Dots: Equatable, Sendable {
    /// The current screen, from 0.
    public var index: Int
    /// How many screens this side of setup has.
    public var count: Int

    /// Creates the dots for one screen of a side of setup.
    ///
    /// - Parameters:
    ///   - index: The current screen, from 0.
    ///   - count: How many screens this side of setup has.
    public init(index: Int, count: Int) {
      self.index = index
      self.count = count
    }
  }

  /// Back, on the left; nil shows none.
  public var back: OnboardingFooterAction?
  /// The next step's button, on the right; nil shows none.
  public var next: OnboardingFooterAction?
  /// The dots; nil shows none.
  public var dots: Dots?

  /// Creates the bar under a screen.
  ///
  /// - Parameters:
  ///   - back: Back, on the left; nil shows none.
  ///   - next: The next step's button, on the right; nil shows none.
  ///   - dots: The dots; nil shows none.
  public init(back: OnboardingFooterAction? = nil, next: OnboardingFooterAction? = nil, dots: Dots? = nil) {
    self.back = back
    self.next = next
    self.dots = dots
  }
}
