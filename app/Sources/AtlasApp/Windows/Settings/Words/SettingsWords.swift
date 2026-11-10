import AtlasUI

/// The Settings window's words that depend on its state: the pages, which also title the window, how the helper
/// stands, the helper's line, the agents' rows, the projects, the alerts it alone asks, the update's answers and its
/// notices. Fixed copy stays in AtlasUI's views; the helper's alerts read the main window's words (``ViewerWords``), so
/// the same question reads the same in both windows.
enum SettingsWords {
  /// A page's title, in the sidebar, the bar and the window's title.
  ///
  /// - Parameter page: The page.
  /// - Returns: "Helper", "General", "Activity", "Accounts", "Privacy", "Integrations" or "About".
  static func page(_ page: SettingsPage) -> String {
    switch page {
    case .helper: "Helper"
    case .general: "General"
    case .activity: "Activity"
    case .accounts: "Accounts"
    case .privacy: "Privacy"
    case .integrations: "Integrations"
    case .about: "About"
    }
  }
}
