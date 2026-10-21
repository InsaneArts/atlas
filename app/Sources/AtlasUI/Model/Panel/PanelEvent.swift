/// What the person did in the panel.
package enum PanelEvent: Equatable, Sendable {
  /// Open Viewer.
  case openViewer
  /// Release Control: revoke every viewer's control at once.
  case revokeControl
  /// Switch to Helper…
  case switchToHelper
  /// Stop Agent: end an agent's input under way in the helper and refuse its next actions.
  case stopAgent
  /// Resume Agent: let agents act in the helper again.
  case resumeAgent
  /// Log In…: lock the screen so the person can log in to the helper.
  case logIn
  /// Recover, from the card or the menu.
  case recover
  /// Log Out Helper…: ask before logging out.
  case askToLogOut
  /// Cancel in the log-out confirmation.
  case cancelLogOut
  /// Log Out in the confirmation.
  case logOut
  /// Log Out Anyway, past an app that held the last log-out up.
  case forceLogOut
  /// Open at Login switched on or off.
  case setOpenAtLogin(Bool)
  /// Getting Started…
  case gettingStarted
  /// Quit Atlas.
  case quit
}
