import AtlasUI

extension SettingsPresenter {
  /// The Helper pane's "Your files" section: the home folder's row once read, private with Undo… or open with Make
  /// Private, its button holding the longer title's width and showing a spinner while its change runs, the sign-ins
  /// switch once known, and what failed; nil until Atlas has said anything.
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
        heldButton: SettingsWords.makePrivate,
        canRun: !files.changing,
        isWorking: files.changingHome
      )
    }
    let status = files.problem.map { SettingsNote(text: $0, kind: .problem) }
    return SettingsFilesState(
      home: home,
      signInCopies: files.signInCopies,
      canChangeSignIns: !files.changing,
      status: status
    )
  }
}
