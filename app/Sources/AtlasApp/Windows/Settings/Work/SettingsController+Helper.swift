import AtlasCommands
import AtlasUI

/// The Helper pane's commands: Check Again diagnoses the helper, and Recover, Switch and Log Out run through the
/// helper's own commands, as the main window's do, each answered with a notice once it ends.
extension SettingsController {
  /// Check Again: `atlas doctor --verbose` diagnoses the helper, once at a time, then the helper's status every window
  /// shares is read again. It shows for at least ``AtlasUI/Motion/Duration/leastWork`` on the injected clock, as the
  /// diagnosis often answers in a fraction of that and a check that flickers past reads as nothing done. All ready
  /// says nothing, since the green marks say it; anything else says what was found under "Ready to act".
  func checkAgain() async {
    guard !session.checking else { return }
    session.checking = true
    let clock = dependencies.clock
    async let least: Void = Self.wait(Motion.Duration.leastWork, on: clock)
    let outcome = await dependencies.commands.run(.doctor)
    let answer = DoctorAnswer.of(outcome)
    if answer == .ready {
      say(nil, in: .helper)
    } else {
      say(SettingsWords.diagnosis(answer), in: .helper, kind: .problem, fromCheck: true)
    }
    await dependencies.links.readStatus()
    await least
    session.checking = false
  }

  /// Waits a step of the duration scale on a clock; a cancelled wait ends at once.
  ///
  /// - Parameters:
  ///   - duration: How long.
  ///   - clock: The clock it runs on.
  nonisolated static func wait(_ duration: Motion.Duration, on clock: any Clock<Duration>) async {
    try? await clock.sleep(for: .seconds(duration.seconds))
  }

  /// Recovers the helper as the panel and the main window do, through the shared commands. Once it worked its alert
  /// says the helper is ready for ``AtlasUI/Motion/Duration/doneHold`` on the injected clock, or until Done, then
  /// closes with the notice; refused, the alert stays, saying why.
  func recover() async {
    session.alertRefusal = nil
    let recovery = await actions.recover()
    if let recovery, recovery.outcome.ok {
      session.alertDone = true
      await doneHold.hold(on: dependencies.clock)
      session.alertDone = false
      session.alert = nil
      say(recovery.notice, in: .helper)
    } else if let recovery {
      session.alertRefusal = recovery.notice
    } else {
      // Another window's Recover runs: the alert shows it running, and the pane says to wait while it does.
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

  /// Closes a login, one at a time: the logins deleted accounts left open, or the helper's own while Atlas there is
  /// silent. macOS's approval dialog is the confirmation. The pane says what came of it, then the helper's status every
  /// window shares is read again.
  ///
  /// - Parameter action: ``AtlasCommands/Action/closeLeftovers`` or ``AtlasCommands/Action/closeHelper``.
  func closeLogin(_ action: Action) async {
    guard !session.closing else { return }
    session.closing = true
    let outcome = await dependencies.commands.run(action)
    session.closing = false
    say(
      outcome.ok ? SettingsWords.closed : Words.sentence(outcome.message),
      in: .helper,
      kind: outcome.ok ? .quiet : .problem
    )
    await dependencies.links.readStatus()
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
      // Another window's Log Out runs: the pane says to wait while it does.
      say(HelperWords.waitForAction, in: .helper, kind: .working)
    }
    await dependencies.links.readStatus()
  }
}
