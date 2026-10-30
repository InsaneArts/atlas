/// What one step of a cleanup does, as the service's `do` names it. A kind this app does not know reads as
/// ``unknown``, so a newer service's step still lists.
public enum CleanupStepKind: String, Equatable, Sendable {
  /// Checks that the files the session's tasks collected out of the helper are still kept, before anything is removed.
  case checkOutputs = "check-outputs"
  /// Quits the apps that run from the session's copies.
  case quit
  /// Stops one background job a task of the session started.
  case stopJob = "stop-job"
  /// Rolls back an environment plan the session applied in the helper.
  case undoPlan = "undo-plan"
  /// Moves an app the session's task installed aside in the helper, which removes it after 7 days.
  case removeInstall = "remove-install"
  /// Removes a copy of a file Atlas made for the session.
  case removeCopy = "remove-copy"
  /// A kind this app does not know.
  case unknown

  /// The kind the service's word names.
  ///
  /// - Parameter word: The step's `do`, or nil when it names none.
  init(word: String?) {
    self = word.flatMap(Self.init(rawValue:)) ?? .unknown
  }
}
