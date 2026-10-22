/// An alert the person asked for in the main window, over the recovery alert the helper's own state calls for.
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
}
