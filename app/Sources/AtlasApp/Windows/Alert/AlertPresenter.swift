import AtlasUI

/// Builds the alert window's alert from what its controller holds. Pure. It reads as the same alert does in the main
/// window and Settings (``HelperAlert/state(overview:progress:)``).
enum AlertPresenter {
  /// The alert as it should read now: running in its action's button, why its command did not run in place of its
  /// sentence, or Recovered once it worked.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The alert.
  static func state(_ snapshot: AlertSnapshot) -> AlertCardState<AlertChoice> {
    snapshot.alert.state(
      overview: snapshot.overview,
      progress: .of(running: snapshot.running, failure: snapshot.failure, done: snapshot.done)
    )
  }
}
