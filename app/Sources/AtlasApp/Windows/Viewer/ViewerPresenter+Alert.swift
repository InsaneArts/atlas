import AtlasKit
import AtlasUI

extension ViewerPresenter {
  /// The alert over the whole window: the one the person asked for, else the recovery alert the helper's state calls
  /// for unless Not Now set it aside.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The alert, or nil for none.
  static func alert(_ snapshot: ViewerSnapshot) -> OrbitAlertState<AlertChoice>? {
    let session = snapshot.session
    if let asked = session.alert {
      return alert(asked, snapshot: snapshot)
    }
    guard case .alert = snapshot.recovery else { return nil }
    return alert(.recover, snapshot: snapshot)
  }

  /// One alert's words and buttons, as every window builds them (``HelperAlert/state(overview:busy:failure:)``);
  /// Restart, which only the main window asks, is a plain alert of its own.
  ///
  /// - Parameters:
  ///   - kind: Which alert.
  ///   - snapshot: What the controller holds.
  /// - Returns: The alert.
  static func alert(_ kind: ViewerAlert, snapshot: ViewerSnapshot) -> OrbitAlertState<AlertChoice> {
    let busy =
      kind == .recover
      ? snapshot.session.recovering || snapshot.running.contains(.recovery) : snapshot.session.alertRunning
    let overview = snapshot.reading.status.overview
    return switch kind {
    case .recover:
      HelperAlert.recover
        .state(overview: overview, busy: busy)
    case .switchToHelper:
      HelperAlert.switchToHelper
        .state(overview: overview, busy: busy)
    case .logOut:
      HelperAlert.logOut
        .state(overview: overview, busy: busy)
    case .logOutAnyway:
      HelperAlert.logOutAnyway
        .state(overview: overview, busy: busy)
    case .restart:
      HelperAlert.plain(
        ViewerWords.restartTitle,
        ViewerWords.restartMessage,
        busy ? ViewerWords.restarting : ViewerWords.restartButton,
        cancel: ViewerWords.cancel,
        busy: busy
      )
    }
  }
}
