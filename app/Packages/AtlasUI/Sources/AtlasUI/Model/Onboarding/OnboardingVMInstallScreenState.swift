/// Atlas making the VM: install macOS, start the VM, set up Atlas inside, connect to Atlas.
public struct OnboardingVMInstallScreenState: Equatable, Sendable {
  /// Where each of the four steps stands, in order.
  public var steps: [StepState]
  /// Why the add is held, under the steps; nil while it goes on.
  public var problem: String?

  /// Creates the install's state.
  ///
  /// - Parameters:
  ///   - steps: Where each of the four steps stands, in order.
  ///   - problem: Why the add is held; nil, by default, while it goes on.
  public init(steps: [StepState], problem: String? = nil) {
    self.steps = steps
    self.problem = problem
  }
}
