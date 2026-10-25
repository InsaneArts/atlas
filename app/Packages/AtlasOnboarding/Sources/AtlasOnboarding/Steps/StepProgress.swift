/// Where one step of a sequence stands: done, current or waiting.
public enum StepProgress: Sendable {
  /// Not reached yet.
  case waiting
  /// The step to do now.
  case current
  /// Behind the person.
  case done
}
