import AtlasKit

/// The viewer's rules that both its presenter and its controller follow: the footer's button and the controller's
/// guard are one rule, and so are the alert shown and the alert Not Now sets aside.
extension ViewerSnapshot {
  /// Whether control can be taken now: nothing asked yet, a fresh reading of a ready helper, not paused, nothing
  /// blocking it and no recovery running.
  var canTakeControl: Bool {
    !session.acquiring && !session.holdingControl && reading.ready && !session.paused && reading.frame != nil
      && !pictureStale && controlBlocked == nil && !session.recovering
  }

  /// Whether the picture is too old to aim input at, as of the snapshot's time: control ends over one, and none is
  /// taken.
  var pictureStale: Bool {
    guard let frameAt = reading.frameAt else { return true }
    return now - frameAt >= Freshness.stale
  }

  /// Why taking control must wait now, as the footer says it, if it must.
  var controlBlocked: String? {
    session.holdingControl ? nil : reading.statusReading.controlBlocked
  }

  /// The recovery alert the status calls for now, set aside or not.
  var offeredRecovery: Recovery? {
    reading.statusReading.recovery(holdingControl: session.holdingControl, recovering: session.recovering)
  }

  /// Where recovery stands as the window shows it: one running always shows; an offered one shows unless Not Now
  /// set it aside for the same reason.
  var recovery: ViewerRecovery {
    guard let offered = offeredRecovery else { return .none }
    if offered.busy || session.recoveryLater != RecoveryKey(offered, status: reading.status) {
      return .alert(offered)
    }
    return .setAside
  }
}
