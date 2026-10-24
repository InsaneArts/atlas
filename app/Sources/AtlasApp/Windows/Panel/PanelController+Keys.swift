import AtlasUI

/// The panel's own shortcuts for its buttons: ⌘. and ⇧⌘P press Stop or Resume and the pause button, as a click would.
extension PanelController {
  /// Presses the button `shortcut` names, as a click would, while it shows and can be pressed.
  ///
  /// - Parameter shortcut: What the key asked for.
  /// - Returns: Whether a button was pressed, so the key goes no further.
  func press(_ shortcut: PanelKeys.Shortcut) -> Bool {
    let state = state
    switch shortcut {
    case .stopOrResume:
      switch state.controls.agent {
      case .stop: send(.stopAgent)
      case .resume(let isEnabled) where isEnabled: send(.resumeAgent)
      case .resume, nil: return false
      }
    case .pausePicture:
      guard state.picture.showsScreen else { return false }
      send(.togglePause)
    case .openAtlas, .openSettings, .quit:
      return false
    }
    return true
  }
}
