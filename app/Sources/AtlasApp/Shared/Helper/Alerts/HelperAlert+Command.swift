extension HelperAlert {
  /// The shared command the alert runs, which Cancel may call off while it still waits to run.
  var command: HelperActions.Command? {
    switch self {
    case .recover: .recovery
    case .switchToHelper: .switchToHelper
    case .logOut, .logOutAnyway: .logOut
    }
  }
}
