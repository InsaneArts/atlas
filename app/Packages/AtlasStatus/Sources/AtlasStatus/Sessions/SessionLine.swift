import Foundation

/// What a host session's line says, as a kind the app words: one rule for the panel's rows, the window's menu and its
/// Details.
public enum SessionLine: Equatable, Sendable {
  /// It has the helper, doing what its newest action says.
  ///
  /// - Parameter says: Its newest action in the service's words, such as "clicking Show Center in Opie"; nil when it
  ///   gives none.
  case acting(says: String?)
  /// It waits its turn while another session has the helper.
  ///
  /// - Parameter seconds: How long it has waited, when the service says.
  case waitingForHelper(seconds: Double?)
  /// It waits until the person recovers the helper.
  case waitingForRecovery
  /// Its action may not have finished, and the helper needs the person.
  ///
  /// - Parameter verb: That action's verb, such as `click`; nil when unknown.
  case mayNotHaveFinished(verb: String?)
  /// It is idle; what it did last.
  ///
  /// - Parameters:
  ///   - says: Its newest action in the service's words, such as "opened Opie".
  ///   - age: How long ago, in seconds, when known.
  case last(says: String, age: Double?)
  /// It has ended.
  ///
  /// - Parameter at: When, when the service says.
  case ended(at: Date?)
  /// It went quiet long ago without ending.
  ///
  /// - Parameter at: When it was last seen, when the service says.
  case lastSeen(at: Date?)
  /// Nothing to say: an idle session with no action read.
  case none
}
