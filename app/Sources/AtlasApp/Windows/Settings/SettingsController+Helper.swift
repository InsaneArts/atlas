import AtlasCommands

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
    say(SettingsWords.diagnosis(answer), in: .helper, problem: answer != .ready)
    await dependencies.links.readStatus()
    session.checking = false
  }

  /// Recovers the helper as the panel and the main window do, through the shared commands.
  func recover() async {
    let recovery = await actions.recover()
    session.alert = nil
    say(recovery?.notice ?? HelperWords.waitForAction, in: .helper, problem: recovery?.outcome.ok != true)
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
    say(
      outcome.map { $0.ok ? ViewerWords.helperLoggedOut : $0.message } ?? HelperWords.waitForAction,
      in: .helper,
      problem: outcome?.ok != true
    )
    await dependencies.links.readStatus()
  }
}
