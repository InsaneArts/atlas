/// What the person has chosen in the window, which the facts cannot say.
public struct OnboardingChoices: Equatable, Sendable {
  /// Past the welcome screen.
  public var started = false
  /// On the account form rather than the list.
  public var creating = false
  /// The agents screen is behind them.
  public var agentsDone = false

  /// Creates choices with nothing chosen.
  public init() {}
}
