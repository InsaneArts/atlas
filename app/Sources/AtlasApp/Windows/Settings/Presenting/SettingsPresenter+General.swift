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
      picture: picture(facts.picture),
      pictureOptions: [
        PopupOption(id: .live, title: SettingsWords.live),
        PopupOption(id: .everyFiveSeconds, title: SettingsWords.everyFiveSeconds),
        PopupOption(id: .paused, title: SettingsWords.paused, isEnabled: facts.canPause || facts.picture == .paused),
      ],
      notify: snapshot.notify
    )
  }

  /// The main window's picture choice as Settings offers it.
  ///
  /// - Parameter choice: The choice.
  /// - Returns: Settings' choice.
  static func picture(_ choice: ViewerPictureChoice) -> SettingsPicture {
    switch choice {
    case .live: .live
    case .everyFiveSeconds: .everyFiveSeconds
    case .paused: .paused
    }
  }

  /// Settings' picture choice as the main window makes it.
  ///
  /// - Parameter picture: Settings' choice.
  /// - Returns: The main window's choice.
  static func choice(_ picture: SettingsPicture) -> ViewerPictureChoice {
    switch picture {
    case .live: .live
    case .everyFiveSeconds: .everyFiveSeconds
    case .paused: .paused
    }
  }
}
