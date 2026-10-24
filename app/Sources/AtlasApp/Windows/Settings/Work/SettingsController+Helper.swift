import AtlasCommands
import AtlasUI

/// The Helper pane's commands: Check Again diagnoses the helper, and Recover, Switch and Log Out run through the
/// helper's own commands, as the main window's do, each answered with a notice once it ends.
extension SettingsController {
  /// Check Again: `atlas doctor --verbose` diagnoses the helper, once at a time, and says what it found, then the
  /// helper's status every window shares is read again.
  func checkAgain() async {
    guard !session.checking else { return }
    session.checking = true
    let outcome = await dependencies.commands.run(.doctor)
    let answer = DoctorAnswer.of(outcome)
    say(SettingsWords.diagnosis(answer), in: .helper, kind: answer == .ready ? .quiet : .problem)
    await dependencies.links.readStatus()
    session.checking = false
  }

  /// Recovers the helper as the panel and the main window do, through the shared commands. Once it worked its alert
  /// says Recovered for ``AtlasUI/Motion/Duration/doneHold`` on the injected clock, then closes with the notice;
  /// refused, the alert stays, saying why.
  func recover() async {
    session.alertRefusal = nil
    let recovery = await actions.recover()
    if let recovery, recovery.outcome.ok {
      session.alertDone = true
      // A hold cut short only shortens the moment Recovered shows; the alert closes either way.
      try? await dependencies.clock.sleep(for: .seconds(Motion.Duration.doneHold.seconds))
      session.alertDone = false
      session.alert = nil
      say(recovery.notice, in: .helper)
    } else if let recovery {
      session.alertRefusal = recovery.notice
    } else {
      // Another window's Recover runs: the alert shows it running, and the pane says to wait while it does (v3 G6).
      say(HelperWords.waitForAction, in: .helper, kind: .working)
    }
    await dependencies.links.readStatus()
  }

  /// Switches this Mac's screen to the helper, saying what to do next or why it could not.
  func switchHelper() async {
    session.alert = nil
    say(Words.locking, in: .helper)
    say(await dependencies.links.switchToHelper(), in: .helper)
  }

  /// Logs the helper out; one an app held up asks again, offering Log Out Anyway.
  ///
  /// - Parameter force: Whether to log out past an app that holds it up.
  func logOut(force: Bool) async {
    let controlled = dependencies.links.status().status.overview.mode == .controlling
    let outcome = await actions.logOut(force: force, revoking: controlled)
    if let outcome, outcome.isLogOutHeld, !force {
      session.alert = .logOutAnyway
      return
    }
    session.alert = nil
    if let outcome {
      say(outcome.ok ? HelperWords.helperLoggedOut : outcome.message, in: .helper, kind: outcome.ok ? .quiet : .problem)
    } else {
      // Another window's Log Out runs: the pane says to wait while it does (v3 G6).
      say(HelperWords.waitForAction, in: .helper, kind: .working)
    }
    await dependencies.links.readStatus()
  }
}
