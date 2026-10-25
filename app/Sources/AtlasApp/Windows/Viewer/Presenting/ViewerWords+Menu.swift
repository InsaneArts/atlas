import AtlasUI

/// The main menu's words: the View and Helper menus' items that change with the window, Find, and each tab's word.
extension ViewerWords {
  /// The View menu's item while Details is hidden, and the Details button's name.
  static let showDetailsItem = "Show Details"
  /// The View menu's item while Details shows, and the Details button's name.
  static let hideDetailsItem = "Hide Details"
  /// The View menu's actual size item.
  static let actualSizeItem = "Actual Size"
  /// The View menu's full screen item, which the system renames while in full screen.
  static let fullScreenItem = "Enter Full Screen"
  /// The Helper menu's item while an agent may act.
  static let stopAgentItem = HelperActionNames.stopAgent
  /// The Helper menu's item once agents are stopped.
  static let resumeAgentItem = HelperActionNames.resumeAgent
  /// The Helper menu's item while this window holds control.
  static let giveBackItem = HelperActionNames.giveBack

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
