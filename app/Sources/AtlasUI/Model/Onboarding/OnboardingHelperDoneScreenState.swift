/// In the helper: set up, so the person switches back.
package struct OnboardingHelperDoneScreenState: Equatable, Sendable {
  /// The headline's second line, naming the account to switch back to.
  package var tail: String

  /// Creates the helper's done screen state.
  ///
  /// - Parameter tail: The headline's second line, naming the account to switch back to.
  package init(tail: String) {
    self.tail = tail
  }
}
