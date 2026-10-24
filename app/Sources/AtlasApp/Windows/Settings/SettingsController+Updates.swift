import AtlasCommands

/// About's update: Check for Updates asks `atlas update --check`, which changes nothing; a newer version found is
/// installed only after the person confirms. One check or install runs at a time.
extension SettingsController {
  /// Asks whether a newer Atlas is published, unless a check or an install runs.
  func checkForUpdates() async {
    guard !session.update.isRunning else { return }
    session.update = .checking
    let outcome = await dependencies.commands.run(.checkForUpdate)
    session.update = .answered(UpdateAnswer.of(outcome))
  }

  /// Asks before installing the newer version the last check found.
  func askInstall() {
    guard let version = session.update.newer else { return }
    ask(.install(version))
  }

  /// Installs the newer version, once the person confirmed. `atlas update` restarts Atlas in the menu bar to finish,
  /// which ends this app with it: that is the update's expected end, so an install that succeeds is usually never
  /// answered here, and one that is says Atlas restarts.
  ///
  /// - Parameter version: The version the person confirmed.
  func install(_ version: String) async {
    session.alert = nil
    guard !session.update.isRunning else { return }
    session.update = .installing(version)
    let outcome = await dependencies.commands.run(.update)
    session.update = outcome.ok ? .installed : .installFailed(Words.sentence(outcome.message))
  }
}
