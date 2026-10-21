/// The button beside Open Viewer.
package enum PanelSecondaryAction: Equatable, Sendable {
  /// Release Control, which revokes every viewer's control: a viewer holds it.
  case revokeControl
  /// Stop Agent: an agent acts, or acted a moment ago, in the helper. It never waits for anything.
  case stopAgent
  /// Resume Agent: the person stopped agents.
  ///
  /// - Parameter isEnabled: Whether it can be pressed: nothing runs.
  case resumeAgent(isEnabled: Bool)
  /// Switch to Helper…
  ///
  /// - Parameter isEnabled: Whether it can be pressed: the helper is known and nothing runs.
  case switchToHelper(isEnabled: Bool)
}
