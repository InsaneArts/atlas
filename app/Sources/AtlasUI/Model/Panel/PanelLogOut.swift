/// Where logging the helper out stands in the panel.
package enum PanelLogOut: Equatable, Sendable {
  /// Nothing to show.
  case none
  /// The confirmation card, before anything runs.
  ///
  /// - Parameter isEnabled: Whether its buttons can be pressed.
  case confirming(isEnabled: Bool)
  /// Log Out Anyway, after an app held the last log-out up.
  ///
  /// - Parameter isEnabled: Whether it can be pressed.
  case held(isEnabled: Bool)
}
