import AtlasKit

/// What Not Now remembers of a recovery alert: the helper's mode and the alert's reason, so a new reason alerts again.
struct RecoveryKey: Equatable {
  /// The helper's mode when the alert was set aside.
  let mode: HelperMode
  /// Why recovery was needed then.
  let reason: String

  /// The key of an alert offered for a status.
  ///
  /// - Parameters:
  ///   - recovery: The alert.
  ///   - status: The status it was offered for.
  init(_ recovery: Recovery, status: ViewerStatus) {
    mode = status.overview.mode
    reason = recovery.reason
  }
}
