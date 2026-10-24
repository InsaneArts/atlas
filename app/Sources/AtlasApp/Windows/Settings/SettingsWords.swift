import AtlasUI

/// The Settings window's words that depend on its state: the tabs, the helper's line, the agents' rows, the projects,
/// the alerts it alone asks, the update's answers and its notices. Fixed copy stays in AtlasUI's views; the helper's
/// alerts read the main window's words (``ViewerWords``), so the same question reads the same in both windows.
enum SettingsWords {
  /// The window's title, which only the Window menu shows.
  static let windowTitle = "Settings"
  /// The tab row, for VoiceOver.
  static let tabsLabel = "Settings"

  /// A tab's word.
  ///
  /// - Parameter tab: The tab.
  /// - Returns: "General", "Helper", "Agents" or "About".
  static func tab(_ tab: SettingsTab) -> String {
    switch tab {
    case .general: "General"
    case .helper: "Helper"
    case .agents: "Agents"
    case .about: "About"
    }
  }
}
