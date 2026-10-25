import AtlasUI

extension SettingsPresenter {
  /// The General pane: Open at Login as the panel keeps it, the picture as the main window keeps it, and the saved
  /// choices. Paused is offered only while the main window is open, since only it pauses.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The pane.
  static func general(_ snapshot: SettingsSnapshot) -> SettingsGeneralState {
    let facts = snapshot.facts
    return SettingsGeneralState(
      opensAtLogin: facts.opensAtLogin.isEnabled,
      canOpenAtLogin: facts.opensAtLogin.isAvailable,
      picture: facts.picture,
      pictureOptions: [
        PopupOption(id: .live, title: SettingsWords.live),
        PopupOption(id: .everyFiveSeconds, title: SettingsWords.everyFiveSeconds),
        PopupOption(id: .paused, title: SettingsWords.paused, isEnabled: facts.canPause || facts.picture == .paused),
      ],
      notify: snapshot.notify
    )
  }
}
