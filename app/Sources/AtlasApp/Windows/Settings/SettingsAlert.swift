/// An alert the person asked for in the Settings window.
enum SettingsAlert: Equatable {
  /// One of the helper's alerts, asked as every window asks it.
  case helper(HelperAlert)
  /// Forget the project in this folder?
  case forget(String)
  /// Install this newer version?
  case install(String)
  /// Delete every picture of agent actions?
  case deletePictures
}
