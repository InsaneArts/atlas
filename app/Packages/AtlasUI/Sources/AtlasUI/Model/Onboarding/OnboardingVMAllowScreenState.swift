/// The person allowing Atlas in the VM's own window: Screen & System Audio Recording, then Accessibility.
public struct OnboardingVMAllowScreenState: Equatable, Sendable {
  /// Where each of the two permissions stands, in order.
  public var steps: [StepState]
  /// Why the add is held, under the steps; nil while it goes on.
  public var problem: String?

  /// Creates the allow screen's state.
  ///
  /// - Parameters:
  ///   - steps: Where each of the two permissions stands, in order.
  ///   - problem: Why the add is held; nil, by default, while it goes on.
  public init(steps: [StepState], problem: String? = nil) {
    self.steps = steps
    self.problem = problem
  }
}
