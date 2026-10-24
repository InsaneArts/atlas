/// The alerts: each asks first, runs its command once however many times its button is pressed, and is answered
/// with a notice once its command ends.
extension SettingsController {
  /// Asks before a command runs.
  ///
  /// - Parameter alert: The alert.
  func ask(_ alert: SettingsAlert) {
    guard !session.alertRunning else { return }
    session.alertRefusal = nil
    session.alert = alert
  }

  /// The alert's way out, unless its command runs. Once a Recover worked it is Done, which ends the hold, so the
  /// alert closes now.
  func cancelAlert() {
    if doneHold.isHolding { return doneHold.end() }
    guard !session.alertRunning else { return }
    session.alertRefusal = nil
    session.alert = nil
  }

  /// The alert's button that goes ahead: its command runs once, whatever else is pressed meanwhile.
  func confirmAlert() async {
    guard !session.alertRunning, let alert = session.alert else { return }
    switch alert {
    case .recover: await run { await self.recover() }
    case .switchToHelper: await run { await self.switchHelper() }
    case .logOut: await run { await self.logOut(force: false) }
    case .logOutAnyway: await run { await self.logOut(force: true) }
    case .forget(let path): await run { await self.forget(path) }
    case .install(let version): await install(version)
    }
  }

  /// Runs an alert's command once, holding its buttons meanwhile.
  ///
  /// - Parameter command: The command.
  private func run(_ command: () async -> Void) async {
    session.alertRunning = true
    await command()
    session.alertRunning = false
  }
}
