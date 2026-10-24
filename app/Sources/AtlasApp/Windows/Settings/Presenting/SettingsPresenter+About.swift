import AtlasUI

extension SettingsPresenter {
  /// The About pane: the version, the one update button as the check goes, and what it says about updates in one
  /// spot (v3 G9).
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The pane.
  static func about(_ snapshot: SettingsSnapshot) -> SettingsAboutState {
    let update = snapshot.session.update
    return SettingsAboutState(
      version: SettingsWords.version(snapshot.facts.version),
      update: button(update),
      status: status(update)
    )
  }

  /// The update button: Install Update… once a newer version is found, each holding a spinner while it runs.
  ///
  /// - Parameter update: Where the check stands.
  /// - Returns: The button.
  static func button(_ update: SettingsUpdate) -> SettingsUpdateButton {
    switch update {
    case .checking: .checking
    case .installing: .installing
    case .answered(.newer): .install
    case .idle, .answered, .installed, .installFailed: .check
    }
  }

  /// What About says about updates: a green check once up to date or updated, the newer version found, the install
  /// under way, the problem line for a failure, or small print while no release is published.
  ///
  /// - Parameter update: Where the check stands.
  /// - Returns: The status, or nil before any answer and while a check runs.
  static func status(_ update: SettingsUpdate) -> SettingsUpdateStatus? {
    switch update {
    case .idle, .checking: nil
    case .answered(.upToDate): .done(SettingsWords.upToDate)
    case .answered(.newer(let version)): .available(SettingsWords.available(version))
    case .installing(let version): .installing(SettingsWords.installingVersion(version))
    case .answered(.failed): .problem(SettingsWords.checkFailed)
    case .answered(.unpublished): .note(SettingsWords.unpublished)
    case .installed: .done(SettingsWords.installed)
    case .installFailed(let message): .problem(message)
    }
  }
}
