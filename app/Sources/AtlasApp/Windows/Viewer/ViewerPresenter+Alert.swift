import AtlasUI

extension ViewerPresenter {
  /// The alert over the whole window: the one the person asked for, else the recovery alert the helper's state calls
  /// for unless Not Now set it aside.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The alert, or nil for none.
  static func alert(_ snapshot: ViewerSnapshot) -> AlertCardState<AlertChoice>? {
    let session = snapshot.session
    if let asked = session.alert {
      return alert(asked, snapshot: snapshot)
    }
    guard case .alert = snapshot.recovery else { return nil }
    return alert(.recover, snapshot: snapshot)
  }

  /// One alert's words and buttons, as every window builds them (``HelperAlert/state(overview:progress:)``);
  /// Restart, which only the main window asks, is a plain alert of its own. Recover shows its refusal and its
  /// success in place.
  ///
  /// - Parameters:
  ///   - kind: Which alert.
  ///   - snapshot: What the controller holds.
  /// - Returns: The alert.
  static func alert(_ kind: ViewerAlert, snapshot: ViewerSnapshot) -> AlertCardState<AlertChoice> {
    let session = snapshot.session
    let progress =
      kind == .recover
      ? AlertProgress.of(
        running: session.recovering || snapshot.running.contains(.recovery),
        failure: session.recoveryRefusal,
        done: session.recoveryDone
      )
      : AlertProgress.of(running: session.alertRunning, failure: nil)
    let overview = snapshot.reading.status.overview
    return switch kind {
    case .recover:
      HelperAlert.recover
        .state(overview: overview, progress: progress)
    case .switchToHelper:
      HelperAlert.switchToHelper
        .state(overview: overview, progress: progress)
    case .logOut:
      HelperAlert.logOut
        .state(overview: overview, progress: progress)
    case .logOutAnyway:
      HelperAlert.logOutAnyway
        .state(overview: overview, progress: progress)
    case .restart:
      HelperAlert.plain(
        ViewerWords.restartTitle,
        ViewerWords.restartMessage,
        ViewerWords.restartButton,
        cancel: HelperWords.cancel,
        phase: progress.phase(running: ViewerWords.restarting, done: ViewerWords.restartTitle)
      )
    }
  }
}
