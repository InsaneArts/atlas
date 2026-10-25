/// Where one numbered step stands.
public enum StepState: Sendable {
  /// Not reached yet.
  case waiting
  /// The step to do now.
  case current
  /// Finished.
  case done
}
