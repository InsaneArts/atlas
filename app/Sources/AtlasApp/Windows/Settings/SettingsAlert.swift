/// An alert the person asked for in the Settings window.
enum SettingsAlert: Equatable {
  /// One of the helper's alerts, asked as every window asks it.
  case helper(HelperAlert)
  /// Forget the project in this folder?
  case forget(String)
  /// Install this newer version?
  case install(String)
  /// Let the helper open the home folder again?
  case openHome
  /// Let agents copy sign-ins into the helper?
  case allowSignIns
  /// Delete every picture of agent actions?
  case deletePictures
  /// Take the account with this id off the list?
  case removeAccount(String)
}
