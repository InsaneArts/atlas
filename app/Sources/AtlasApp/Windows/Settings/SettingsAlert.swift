/// An alert the person asked for in the Settings window.
enum SettingsAlert: Equatable {
  /// Recover the helper?
  case recover
  /// Switch to the helper?
  case switchToHelper
  /// Log out the helper?
  case logOut
  /// The helper didn't log out: Log Out Anyway, after an app held a log-out up.
  case logOutAnyway
  /// Forget the project in this folder?
  case forget(String)
  /// Install this newer version?
  case install(String)
}
