extension StepList {
  /// One step placed in the list: its number, and where its neighbours put the rail.
  struct Placed: Identifiable, Equatable, Sendable {
    /// Its number, from 1, which is also its identity.
    let id: Int
    /// The step.
    let step: Step
    /// Where the step above stands; nil for the first.
    let above: StepState?
    /// Whether a step follows.
    let continues: Bool
  }
}
