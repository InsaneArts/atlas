import AtlasUI

extension SettingsPresenter {
  /// The About pane: the version, the update button as the check goes, and what the last check or install said.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The pane.
  static func about(_ snapshot: SettingsSnapshot) -> SettingsAboutState {
    let update = snapshot.session.update
    return SettingsAboutState(
      version: SettingsWords.version(snapshot.facts.version),
      update: button(update),
      answer: answer(update)
    )
  }

  /// The update button: Install Update… once a newer version is found, each waiting while it runs.
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

  /// What the last check or install said.
  ///
  /// - Parameter update: Where the check stands.
  /// - Returns: The line, or nil before any.
  static func answer(_ update: SettingsUpdate) -> String? {
    switch update {
    case .idle, .checking: nil
    case .answered(.upToDate): SettingsWords.upToDate
    case .answered(.newer(let version)), .installing(let version): SettingsWords.available(version)
    case .answered(.failed): SettingsWords.checkFailed
    case .answered(.unpublished): SettingsWords.unpublished
    case .installed: SettingsWords.installed
    case .installFailed(let message): message
    }
  }
}
