import AtlasUI

extension SettingsPresenter {
  /// The Helper pane's "Your files" section: the home folder's row once read, private with Undo… or open with Make
  /// Private, the sign-ins switch once known, and a change's spinner or failure; nil until Atlas has said anything.
  ///
  /// - Parameter files: What the controller knows of the person's files.
  /// - Returns: The section, or nil for none yet.
  static func files(_ files: SettingsFiles) -> SettingsFilesState? {
    guard files.home != nil || files.signInCopies != nil || files.problem != nil else { return nil }
    let home = files.home.map { home in
      SettingsFilesRow(
        isPrivate: home.private,
        title: home.private ? SettingsWords.homePrivate : SettingsWords.homeOpen(home.readable),
        detail: home.private ? SettingsWords.homePrivateDetail : SettingsWords.homeOpenDetail(home.examples),
        button: home.private ? SettingsWords.undo : SettingsWords.makePrivate,
        canRun: !files.changing
      )
    }
    let status =
      files.changing
      ? SettingsNote(text: SettingsWords.changingFiles, kind: .working)
      : files.problem.map { SettingsNote(text: $0, kind: .problem) }
    return SettingsFilesState(
      home: home,
      signInCopies: files.signInCopies,
      canChangeSignIns: !files.changing,
      status: status
    )
  }
}
