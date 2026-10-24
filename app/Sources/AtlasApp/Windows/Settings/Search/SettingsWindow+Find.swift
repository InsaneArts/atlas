import AppKit

/// Settings answers the existing Find action only while it is the responder-chain destination.
extension SettingsWindow: FindMenuActions {
  /// Focuses Settings search without installing another shortcut.
  ///
  /// - Parameter sender: The existing menu item.
  func findInTab(_ sender: Any?) { controller.send(.find) }
}
