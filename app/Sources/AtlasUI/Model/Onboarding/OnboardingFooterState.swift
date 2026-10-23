/// The bar under a setup screen. Its defaults show no buttons and no dots, as the permission panel and the first reading have.
package struct OnboardingFooterState: Equatable, Sendable {
  /// Where the screen sits among the dots in the middle of the bar.
  package struct Dots: Equatable, Sendable {
    /// The current screen, from 0.
    package var index: Int
    /// How many screens this side of setup has.
    package var count: Int

    /// Creates the dots for one screen of a side of setup.
    ///
    /// - Parameters:
    ///   - index: The current screen, from 0.
    ///   - count: How many screens this side of setup has.
    package init(index: Int, count: Int) {
      self.index = index
      self.count = count
    }
  }

  /// Back, on the left; nil shows none.
  package var back: OnboardingFooterAction?
  /// The next step's button, on the right; nil shows none.
  package var next: OnboardingFooterAction?
  /// The dots; nil shows none.
  package var dots: Dots?

  /// Creates the bar under a screen.
  ///
  /// - Parameters:
  ///   - back: Back, on the left; nil shows none.
  ///   - next: The next step's button, on the right; nil shows none.
  ///   - dots: The dots; nil shows none.
  package init(back: OnboardingFooterAction? = nil, next: OnboardingFooterAction? = nil, dots: Dots? = nil) {
    self.back = back
    self.next = next
    self.dots = dots
  }
}
