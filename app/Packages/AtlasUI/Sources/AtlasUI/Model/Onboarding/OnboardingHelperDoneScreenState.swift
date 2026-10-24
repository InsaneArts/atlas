/// In the helper: set up, so the person switches back.
public struct OnboardingHelperDoneScreenState: Equatable, Sendable {
  /// The headline's second line, naming the account to switch back to.
  public var tail: String

  /// Creates the helper's done screen state.
  ///
  /// - Parameter tail: The headline's second line, naming the account to switch back to.
  public init(tail: String) {
    self.tail = tail
  }
}
