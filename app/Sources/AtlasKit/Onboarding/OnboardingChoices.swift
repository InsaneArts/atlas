/// What the person has chosen in the window, which the facts cannot say.
package struct OnboardingChoices: Equatable, Sendable {
  /// Past the welcome screen.
  package var started = false
  /// On the account form rather than the list.
  package var creating = false
  /// The agents screen is behind them.
  package var agentsDone = false

  /// Creates choices with nothing chosen.
  package init() {}
}
