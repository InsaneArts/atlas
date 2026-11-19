extension StepList {
  /// One step's words and where it stands.
  struct Step: Equatable, Sendable {
    /// What to do.
    let title: String
    /// Where or how; empty for none.
    let detail: String
    /// Where the step stands.
    let state: StepState
  }
}
