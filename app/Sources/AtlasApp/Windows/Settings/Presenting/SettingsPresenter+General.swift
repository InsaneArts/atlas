import AtlasUI

extension SettingsPresenter {
  /// The General pane: Open at Login as the panel keeps it, the menu bar's glyph as its item draws it, the picture as
  /// the main window keeps it, the helper's sound as `atlas sound` says, the pictures of agent actions as Atlas's
  /// settings keep them with how many this Mac holds, and the saved choices. Paused is offered only while the main window is open, since only it pauses. The
  /// helper counts as muted until it says otherwise, as Atlas mutes it unless told not to; a mute the person wants and
  /// the helper cannot make says why, as does a change that did not reach the helper. Days of three or seven read as
  /// their choice unless the person chose Custom….
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The pane.
  static func general(_ snapshot: SettingsSnapshot) -> SettingsGeneralState {
    let facts = snapshot.facts
    let pictures = snapshot.pictures
    let days = pictures.shown.keepDays
    let keepFor =
      SettingsKeepFor.allCases
      .first { !pictures.customDays && $0.days == days } ?? .custom
    return SettingsGeneralState(
      opensAtLogin: facts.opensAtLogin.isEnabled,
      canOpenAtLogin: facts.opensAtLogin.isAvailable,
      menuBarIcon: facts.menuBarIcon,
      picture: facts.picture,
      pictureOptions: [
        PopupOption(id: .live, title: SettingsWords.live),
        PopupOption(id: .everyFiveSeconds, title: SettingsWords.everyFiveSeconds),
        PopupOption(id: .paused, title: SettingsWords.paused, isEnabled: facts.canPause || facts.picture == .paused),
      ],
      muteHelper: snapshot.sound.isMuted,
      soundProblem: snapshot.sound.problem,
      picturesBeforeActions: pictures.shown.isOn,
      keepFor: keepFor,
      keepDays: days,
      typedText: pictures.shown.typedTextLimit == nil ? .full : .limited,
      typedTextLimit: pictures.shown.typedTextLimit ?? SettingsGeneralState.typedTextDefault,
      storedPictures: pictures.stored.map { SettingsWords.storedPictures(count: $0.count, bytes: $0.bytes) } ?? "",
      // What a picture's details keep, typed text among them, may be left beside none.
      canDeletePictures: pictures.stored.map { $0.count > 0 || $0.bytes > 0 } ?? false,
      notify: snapshot.notify,
      notice: notice(snapshot, in: .general, busy: false)
    )
  }
}
