/// Why the panel offers Recover on a card of its own.
package enum PanelRecovery: Equatable, Sendable {
  /// The last action's outcome is unknown.
  ///
  /// - Parameter detail: Why recovery is needed.
  case unfinished(String)
  /// The helper's input stopped or lost its display; Recover starts it again.
  ///
  /// - Parameter detail: What happened.
  case notReady(String)
}
