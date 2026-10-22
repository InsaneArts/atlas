import AtlasKit
import AtlasUI

/// Builds the alert window's alert from what its controller holds. Pure. It reads as the same alert does in the main
/// window and Settings (``HelperAlert/state(overview:busy:failure:)``).
enum AlertPresenter {
  /// The alert as it should read now: why its command did not run in place of its sentence, and its action's word
  /// while it runs.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The alert.
  static func state(_ snapshot: AlertSnapshot) -> OrbitAlertState<AlertChoice> {
    snapshot.alert.state(overview: snapshot.overview, busy: snapshot.running, failure: snapshot.failure)
  }
}
