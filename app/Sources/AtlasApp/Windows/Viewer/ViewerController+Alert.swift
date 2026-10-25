import AtlasUI

/// The alerts: Recover, Switch, Log Out, Log Out Anyway and Restart, each asked first, run once however many times
/// its button is pressed, and answered with a notice once its command ends. Recover answers in place: refused, the
/// alert stays and says why; worked, it says the helper is ready for ``AtlasUI/Motion/Duration/doneHold`` on the
/// injected clock, or until Done, before it closes.
extension ViewerController {
  /// The alert over the window now: the one asked for, else the recovery alert the helper's state calls for.
  var shownAlert: ViewerAlert? {
    if let asked = session.alert { return asked }
    if case .alert = snapshot.recovery { return .recover }
    return nil
  }

  /// Asks before a command runs. Answering Atlas is not acting in the helper, so control goes back first. While an
  /// alert's command runs here, the request is not dropped unseen: the notice says to try again once it ends.
  ///
  /// - Parameter alert: The alert.
  func ask(_ alert: ViewerAlert) {
    guard !session.alertRunning, !session.recovering else { return say(HelperWords.waitForAction) }
    control.releaseControl()
    if alert == .recover {
      session.recoveryLater = nil
      session.recoveryRefusal = nil
    }
    session.alert = alert
  }

  /// A status was shown: once nothing needs recovery, the recovery set aside is forgotten, and so is a refusal its
  /// alert no longer shows.
  func forgetRecoveryOnceNoneNeeded() {
    // Not Now covers one incident: once nothing needs recovery, a later one alerts again.
    if snapshot.offeredRecovery == nil {
      session.recoveryLater = nil
      // A refusal belongs to the alert it answered: once nothing needs recovery it is gone with it.
      if session.alert != .recover { session.recoveryRefusal = nil }
    }
  }

  /// The alert's way out: Cancel, or Not Now, which sets the recovery alert aside until its reason changes. Once a
  /// Recover worked it is Done, which ends the hold, so the alert closes now.
  func cancelAlert() {
    if doneHold.isHolding { return doneHold.end() }
    guard !session.alertRunning, !session.recovering, let shown = shownAlert else { return }
    session.alert = nil
    session.recoveryRefusal = nil
    if shown == .recover, let offered = snapshot.offeredRecovery {
      session.recoveryLater = RecoveryKey(offered, status: reading.status)
    }
  }

  /// The alert's button that goes ahead: its command runs once, whatever else is pressed meanwhile.
  func confirmAlert() async {
    switch shownAlert {
    case .recover: await recover()
    case .switchToHelper: await run { await self.switchHelper() }
    case .logOut: await run { await self.logOut(force: false) }
    case .logOutAnyway: await run { await self.logOut(force: true) }
    case .restart: await run { await self.restart() }
    case nil: return
    }
  }

  /// Recovers the helper as the panel does, through the shared commands. Never while holding control; never repeats
  /// an action. Once it worked the alert stays up, saying the helper is ready, through the hold or until Done,
  /// whatever the helper's state says meanwhile, then closes with the notice; refused, it stays, saying why.
  func recover() async {
    guard !session.recovering, !holdingControl else { return }
    session.recovering = true
    session.recoveryLater = nil
    session.recoveryRefusal = nil
    let recovery = await actions.recover()
    if let recovery, recovery.outcome.ok {
      session.alert = .recover
      session.recoveryDone = true
      say(nil)
      await doneHold.hold(on: dependencies.clock)
      session.recoveryDone = false
      session.alert = nil
      // A notice said during the hold, such as to wait for another alert, stays.
      if session.notice == nil { say(recovery.notice) }
    } else if let recovery {
      session.recoveryRefusal = recovery.notice
    } else {
      // Another window's Recover runs: the alert shows it running, and the notice says to wait.
      say(HelperWords.waitForAction)
    }
    session.recovering = false
    // Read the outcome directly: a paused picture makes no requests.
    await readingLoop.readStatus()
    readingLoop.refresh()
  }

  /// Runs an asked alert's command once, holding its buttons meanwhile, then reads the outcome.
  ///
  /// - Parameter command: The command.
  private func run(_ command: () async -> Void) async {
    guard !session.alertRunning else { return }
    session.alertRunning = true
    await command()
    session.alertRunning = false
    await readingLoop.readStatus()
    readingLoop.refresh()
  }

  /// Switches this Mac's screen to the helper, saying what to do next or why it could not.
  private func switchHelper() async {
    session.alert = nil
    guard let switchToHelper = dependencies.switchToHelper else { return }
    say(Words.locking)
    say(await switchToHelper())
  }

  /// Logs the helper out; one an app held up asks again, offering Log Out Anyway.
  ///
  /// - Parameter force: Whether to log out past an app that holds it up.
  private func logOut(force: Bool) async {
    let controlled = reading.status.overview.mode == .controlling
    let outcome = await actions.logOut(force: force, revoking: controlled)
    if let outcome, outcome.isLogOutHeld, !force {
      session.alert = .logOutAnyway
      return
    }
    session.alert = nil
    say(outcome.map { $0.ok ? HelperWords.helperLoggedOut : $0.message } ?? HelperWords.waitForAction)
  }

  /// Restarts the helper.
  private func restart() async {
    let outcome = await actions.restart()
    session.alert = nil
    say(outcome.map { $0.ok ? ViewerWords.helperRestarted : $0.message } ?? HelperWords.waitForAction)
  }
}
