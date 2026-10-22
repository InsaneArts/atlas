/// An alert one window asks another to show before one of the helper's commands runs: the menu bar panel asks, and
/// `AppController` shows it in the main window when it is open, else in the alert window.
enum HelperAlert: Equatable, Sendable {
  /// Recover the helper?
  case recover
  /// Switch to the helper?
  case switchToHelper
  /// Log out the helper?
  case logOut
  /// The helper didn't log out: Log Out Anyway, after an app held a log-out up.
  case logOutAnyway
}
