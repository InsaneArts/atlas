/// The footer's button for agents: Stop Agent while one acts, Resume Agent once the person stopped them.
package enum ViewerAgentButton: Equatable, Sendable {
  /// Stop Agent. It never waits for anything.
  case stop
  /// Resume Agent.
  ///
  /// - Parameter isEnabled: Whether it can be pressed: nothing else is asked of the service.
  case resume(isEnabled: Bool)
}
