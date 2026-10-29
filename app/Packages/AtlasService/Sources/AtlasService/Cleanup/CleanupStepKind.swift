/// What one step of a cleanup does, as the service's `do` names it. A kind this app does not know reads as
/// ``unknown``, so a newer service's step still lists.
public enum CleanupStepKind: String, Equatable, Sendable {
  /// Quits the apps that run from the session's copies.
  case quit
  /// Rolls back an environment plan the session applied in the helper.
  case undoPlan = "undo-plan"
  /// Moves an app the session's task installed to the helper's Trash.
  case removeInstall = "remove-install"
  /// Moves a copy of an app Atlas made for the session to the person's Trash.
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
