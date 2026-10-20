/// The button beside Open Viewer.
package enum PanelSecondaryAction: Equatable, Sendable {
  /// Release Control, which revokes every viewer's control: a viewer holds it.
  case revokeControl
  /// Switch to Helper…
  ///
  /// - Parameter isEnabled: Whether it can be pressed: the helper is known and nothing runs.
  case switchToHelper(isEnabled: Bool)
}
