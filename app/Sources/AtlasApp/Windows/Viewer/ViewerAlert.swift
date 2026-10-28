/// An alert the person asked for in the main window, over the recovery alert the helper's state calls for.
enum ViewerAlert: Equatable {
  /// Recover the helper?
  case recover
  /// Switch to the helper?
  case switchToHelper
  /// Log out the helper?
  case logOut
  /// The helper didn't log out: Log Out Anyway, after an app held a log-out up.
  case logOutAnyway
  /// Restart the helper?
  case restart
  /// Clean up a session's work? Asked once what it takes was read.
  ///
  /// - Parameters:
  ///   - session: The session's key, whose work Clean Up cleans up.
  ///   - steps: What it does, a line each, such as "Quits Probe".
  ///   - warning: That the session may still be working; nil when it is not.
  case cleanup(session: String, steps: [String], warning: String?)
}
