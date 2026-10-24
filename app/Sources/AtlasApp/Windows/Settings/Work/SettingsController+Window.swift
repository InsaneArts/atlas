import AtlasUI

/// Session navigation updates the title and remembered root without resizing the window.
extension SettingsController {
  /// Moves through existing visits without appending or starting a command.
  ///
  /// - Parameter forward: Whether to move forward rather than back.
  func moveInHistory(forward: Bool) {
    let before = session.navigation.destination
    if forward {
      session.navigation.forward()
    } else {
      session.navigation.back()
    }
    guard let destination = session.navigation.destination, destination != before else { return }
    dependencies.preferences.saveTab(destination.root.rawValue)
    nameWindow?("Settings — \(SettingsWords.tab(session.tab))")
  }
}
