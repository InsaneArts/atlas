/// The recovery alert: whether recovery is running, else why it is offered.
package enum Recovery: Equatable, Sendable {
  /// Recovery is running.
  case running
  /// An action's outcome is unknown, for this reason.
  case unknownOutcome(StatusReading.Reason)
  /// The helper is not ready for a reason Recover fixes.
  case notReady(StatusReading.Reason)

  /// Whether recovery is running.
  package var busy: Bool { self == .running }
}
