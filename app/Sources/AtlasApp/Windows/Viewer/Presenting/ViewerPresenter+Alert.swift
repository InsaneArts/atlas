import AtlasUI

extension ViewerPresenter {
  /// The alert over the whole window: the one the person asked for, whose way out is Cancel, else the recovery alert
  /// the helper's state calls for unless Not Now set it aside, whose way out is Not Now.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The alert, or nil for none.
  static func alert(_ snapshot: ViewerSnapshot) -> AlertCardState<AlertChoice>? {
    let session = snapshot.session
    if let asked = session.alert {
      return alert(asked, snapshot: snapshot, askedByPerson: true)
    }
    guard case .alert = snapshot.recovery else { return nil }
    return alert(.recover, snapshot: snapshot)
  }

  /// One alert's words and buttons, as every window builds them
  /// (``HelperAlert/state(overview:progress:askedByPerson:)``); Restart, which only the main window asks, is a
  /// destructive alert of its own, since anything an agent is doing in the helper stops: Cancel is the default and
  /// Restart red. Recover shows its refusal and its success in place.
  ///
  /// - Parameters:
  ///   - kind: Which alert.
  ///   - snapshot: What the controller holds.
  ///   - askedByPerson: Whether the person asked for it, so its way out is Cancel; false for the recovery alert Atlas
  ///     raises by itself, by default.
  /// - Returns: The alert.
  static func alert(
    _ kind: ViewerAlert,
    snapshot: ViewerSnapshot,
    askedByPerson: Bool = false
  ) -> AlertCardState<AlertChoice> {
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
        .state(overview: overview, progress: progress, askedByPerson: askedByPerson)
    case .switchToHelper:
      HelperAlert.switchToHelper
        .state(overview: overview, progress: progress, askedByPerson: askedByPerson)
    case .logOut:
      HelperAlert.logOut
        .state(overview: overview, progress: progress, askedByPerson: askedByPerson)
    case .logOutAnyway:
      HelperAlert.logOutAnyway
        .state(overview: overview, progress: progress, askedByPerson: askedByPerson)
    case .restart:
      HelperAlert.destructive(
        ViewerWords.restartTitle,
        ViewerWords.restartMessage,
        ViewerWords.restartButton,
        phase: progress.phase(
          running: ViewerWords.restarting,
          done: ViewerWords.restartTitle,
          doneMessage: ViewerWords.restartMessage
        )
      )
    }
  }
}
