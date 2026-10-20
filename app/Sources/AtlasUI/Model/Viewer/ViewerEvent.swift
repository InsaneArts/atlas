/// What the person did in the viewer window.
package enum ViewerEvent: Equatable, Sendable {
  /// The sidebar's toggle, or ⌥⌘I.
  case toggleSidebar
  /// The pause button, or ⇧⌘P.
  case togglePause
  /// Take Control, or Control-Shift-Escape while watching.
  case takeControl
  /// Give Back, or Control-Shift-Escape while the desktop does not have the keyboard.
  case giveBack
  /// Recover in the recovery alert.
  case recover
  /// Not Now in the recovery alert: set it aside until its reason changes.
  case recoverLater
  /// Recover… in the footer: show the alert set aside.
  case showRecovery
  /// Keys: offer the keys the Mac keeps for itself.
  case showKeys
  /// A refresh rate picked in the sidebar.
  case setRate(ViewerPictureRate)
  /// Log In… on a logged-out helper's stage.
  case logIn
}
