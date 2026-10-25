import AtlasUI

/// Session navigation updates the title and remembered root without resizing the window.
extension SettingsController {
  /// Moves through existing visits and refreshes the destination's read-only content.
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
    enteredDestination()
  }

  /// Remembers the destination, updates its title and refreshes its read-only content on every entry.
  func enteredDestination() {
    guard let destination = session.navigation.destination else { return }
    dependencies.preferences.saveTab(destination.root.rawValue)
    nameWindow?("Settings — \(destination.title)")
    if session.tab == .agents { readAgents() }
  }
}
