/// How a cleanup of a session's work ended, as its steps say, for the notice after it.
public enum CleanupEnding: Equatable, Sendable {
  /// Every step is done, or kept on purpose.
  case cleanedUp
  /// The session left nothing to clean up after all.
  case nothingToCleanUp
  /// A step failed or did not run.
  case unfinished
  /// Whether its last step finished is not known, which recovery settles.
  case uncertain
  /// It was held back before every step ran: a collected copy no longer kept (``CleanupStep/Reason/outputNotKept``),
  /// a helper that could not be reached to stop a job (``CleanupStep/Reason/daemon``), or otherwise agents stopped,
  /// whatever reason, if any, its first held step gave.
  ///
  /// - Parameter reason: Why it was held back.
  case blocked(CleanupStep.Reason?)
}
