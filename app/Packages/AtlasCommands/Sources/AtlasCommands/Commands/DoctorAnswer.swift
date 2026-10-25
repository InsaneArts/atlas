import AtlasService

/// How Check Again's diagnosis ended: the helper is ready, it needs these steps, or the diagnosis did not run.
public enum DoctorAnswer: Equatable, Sendable {
  /// Every check passed.
  case ready
  /// Checks failed: what to do, first step first.
  case needs([String])
  /// The diagnosis did not answer, saying why.
  case failed(String)

  /// The answer `atlas doctor --verbose` gave. The command exits non-zero whenever the helper is not ready, so its
  /// report is read whatever the exit said: a verdict present is the answer, and only a reply without one failed. A
  /// helper that is not ready with no step named still needs the person.
  ///
  /// - Parameter outcome: The command's outcome.
  /// - Returns: Ready, the steps it needs, or why it failed.
  public static func of(_ outcome: Outcome) -> DoctorAnswer {
    guard let report = outcome.decode(DoctorReport.self), let ready = report.ready else {
      return .failed(outcome.ok ? "" : outcome.message)
    }
    return ready ? .ready : .needs(report.nextSteps ?? [])
  }
}
