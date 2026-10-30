import AtlasUI

/// Builds the alert window's alert from what its controller holds. Pure. It reads as the same alert does in the main
/// window and Settings (``HelperAlert/state(overview:progress:askedByPerson:)``), and the person always asked for it,
/// from the panel or a menu, so its way out is Cancel.
enum AlertPresenter {
  /// The alert as it should read now: running in its action's button, why its command did not run in place of its
  /// sentence, or Recovered once it worked.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The alert.
  static func state(_ snapshot: AlertSnapshot) -> AlertCardState<AlertChoice> {
    snapshot.alert.state(
      overview: snapshot.overview,
      progress: .of(running: snapshot.running, failure: snapshot.failure, done: snapshot.done),
      askedByPerson: true,
      onConsole: snapshot.isOnConsole
    )
  }
}
