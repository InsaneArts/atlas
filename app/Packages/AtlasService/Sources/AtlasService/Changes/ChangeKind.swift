/// What the service's feed says to read again: one of the things a view shows.
public enum ChangeKind: String, CaseIterable, Sendable {
  /// What agents did, and the pictures taken around it.
  case activity
  /// The host sessions.
  case sessions
  /// The questions Atlas asks the person, and what they allowed.
  case asks
  /// The helper's state.
  case status
  /// Who holds control.
  case control
  /// The desktops Atlas serves.
  case desktops

  /// What an event of the kind named `name` says to read again: a kind this app does not know may say anything.
  ///
  /// - Parameter name: The kind's name, as the feed gives it.
  /// - Returns: The kinds to read again.
  static func named(_ name: String) -> Set<ChangeKind> {
    ChangeKind(rawValue: name).map { [$0] } ?? Set(allCases)
  }
}
