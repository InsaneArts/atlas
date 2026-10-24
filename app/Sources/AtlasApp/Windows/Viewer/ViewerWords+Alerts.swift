/// The helper alerts' words: recovery, the switch, log out and its forced form, and restart, each with its title,
/// sentence and buttons.
extension ViewerWords {
  /// The way out of an alert.
  static let cancel = "Cancel"
  /// The recovery alert's way out, which sets it aside.
  static let notNow = "Not Now"
  /// The recovery alert's title.
  static let recoverTitle = "Recover the helper?"
  /// The recovery alert's sentence while an action's outcome is unknown.
  static let recoverUnfinished =
    "An action may not have finished. Atlas checks the helper and starts its mouse and keyboard again."
  /// The recovery alert's sentence while the helper's input stopped.
  static let recoverInput = "Mouse and keyboard stopped in the helper. Atlas checks the helper and starts them again."
  /// The recovery alert's sentence when it was asked for otherwise.
  static let recoverAsked = "Atlas checks the helper and starts it again. No action runs twice."
  /// The recovery alert's button.
  static let recover = "Recover"
  /// The recovery alert's button while recovery runs.
  static let recovering = "Recovering…"
  /// The switch alert's title.
  static let switchTitle = "Switch to the helper?"
  /// The switch alert's sentence.
  static let switchMessage =
    "Your screen locks so you can log in to the helper. Everything here keeps running, and you can switch back any time."
  /// The switch alert's button.
  static let switchButton = "Switch"
  /// The switch alert's button while it runs.
  static let switching = "Switching…"
  /// The log-out alert's title.
  static let logOutTitle = "Log out the helper?"
  /// The log-out alert's sentence.
  static let logOutMessage = "Its apps quit, and agents can't use it until you log in to it again."
  /// The log-out alert's button.
  static let logOutButton = "Log Out"
  /// The log-out alert's button while it runs.
  static let loggingOut = "Logging Out…"
  /// The forced log-out alert's title, after an app held a log-out up.
  static let logOutHeldTitle = "The helper didn't log out"
  /// The forced log-out alert's sentence.
  static let logOutHeldMessage =
    "An app in the helper is holding it up. Log Out Anyway quits it, and anything unsaved there is lost."
  /// The forced log-out alert's button.
  static let logOutAnyway = "Log Out Anyway"
  /// The restart alert's title.
  static let restartTitle = "Restart the helper?"
  /// The restart alert's sentence.
  static let restartMessage = "Atlas starts its service in the helper again. Anything an agent is doing there stops."
  /// The restart alert's button.
  static let restartButton = "Restart"
  /// The restart alert's button while it runs.
  static let restarting = "Restarting…"
}
