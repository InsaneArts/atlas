import AtlasUI

/// The main menu's words: the View and Helper menus' items that change with the window, Stop's and Resume's
/// tooltips, and each tab's word.
extension ViewerWords {
  /// The View menu's item while Details is hidden.
  static let showDetailsItem = "Show Details"
  /// The View menu's item while Details shows.
  static let hideDetailsItem = "Hide Details"
  /// The View menu's item while pictures arrive.
  static let pausePictureItem = "Pause Picture"
  /// The View menu's item while the picture is paused.
  static let resumePictureItem = "Resume Picture"
  /// The View menu's actual size item.
  static let actualSizeItem = "Actual Size"
  /// The View menu's full screen item, which the system renames while in full screen.
  static let fullScreenItem = "Enter Full Screen"
  /// The Helper menu's item while an agent may act.
  static let stopAgentItem = "Stop Agent"
  /// Stop's tooltip, with its shortcut.
  static let stopAgentHelp = "Stop what the agent is doing in the helper, until you resume (⌘.)"
  /// Resume's tooltip, with its shortcut.
  static let resumeAgentHelp = "Let agents act in the helper again (⌘.)"
  /// The Helper menu's item once agents are stopped.
  static let resumeAgentItem = "Resume Agent"

  /// A tab's word, in the top row and the View menu.
  ///
  /// - Parameter tab: The tab.
  /// - Returns: "Screen", "Activity", "Apps" or "Screenshots".
  static func tab(_ tab: ViewerTab) -> String {
    switch tab {
    case .screen: "Screen"
    case .activity: "Activity"
    case .apps: "Apps"
    case .screenshots: "Screenshots"
    }
  }
}
