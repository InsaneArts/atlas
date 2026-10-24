import AtlasKit

/// The main window's rules that both its presenter and its controller follow: the button drawn and the controller's
/// guard are one rule, and so are the alert shown and the alert Not Now sets aside.
extension ViewerSnapshot {
  /// Whether control can be taken now: nothing asked yet, no acquisition or release unanswered anywhere in the app,
  /// none of the helper's commands running from any window, a fresh reading of a ready helper, not paused and
  /// nothing blocking it. An agent at work blocks nothing: the service answers busy while one of its actions runs,
  /// and the controller asks again once.
  var canTakeControl: Bool {
    !session.acquiring && !session.holdingControl && !controlBusy && running.isEmpty && !session.alertRunning
      && !session.recovering && reading.ready && !session.paused && reading.frame != nil && !pictureStale
      && controlBlocked == nil
  }

  /// Whether the picture is too old to aim input at, as of the snapshot's time: control ends over one, and none is
  /// taken.
  var pictureStale: Bool {
    guard let frameAt = reading.frameAt else { return true }
    return now - frameAt >= Freshness.stale
  }

  /// Why taking control must wait now, if it must.
  var controlBlocked: StatusReading.ControlBlock? {
    session.holdingControl ? nil : reading.statusReading.controlBlocked
  }

  /// The recovery alert the status calls for now, set aside or not: busy while this window recovers, and, where
  /// recovery would be offered, while a Recover or a restart runs from any other window.
  var offeredRecovery: Recovery? {
    let reading = reading.statusReading
    let offered = reading.recovery(holdingControl: session.holdingControl, recovering: session.recovering)
    guard offered != nil, running.contains(.recovery) else { return offered }
    return reading.recovery(holdingControl: session.holdingControl, recovering: true)
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

  /// The mode as this window shows it (``ViewerPresenter/mode(reading:session:)``).
  var mode: Mode { ViewerPresenter.mode(reading: reading, session: session) }

  /// Which of the helper's commands the window offers now: its menus' items, the alert's buttons and the empty
  /// messages' buttons all follow it.
  var helperMenu: HelperMenuState {
    let overview = reading.status.overview
    return HelperMenuState(
      canRecover: offeredRecovery != nil && !session.recovering && !running.contains(.recovery),
      canRestart: overview.mode.answers && !running.contains(.recovery),
      canSwitch: !running.contains(.switchToHelper),
      canLogOut: overview.mode.allowsLogOut && !running.contains(.logOut)
    )
  }
}
