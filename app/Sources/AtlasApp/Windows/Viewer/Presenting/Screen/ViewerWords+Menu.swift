import AtlasUI

/// The menus' words: the View, Session and Helper menus' items, those of a session's and the helper line's menus, and
/// each place's word.
extension ViewerWords {
  /// The Session menu.
  static let sessionMenu = "Session"
  /// The Session menu's item that shows every session's work.
  static let allSessionsItem = "All Sessions"
  /// The Session menu's item that chooses the session over the one chosen.
  static let previousSessionItem = "Previous Session"
  /// The Session menu's item that chooses the session under the one chosen.
  static let nextSessionItem = "Next Session"
  /// The Session menu's item, ticked while Ended lists every session that ended.
  static let showEndedItem = "Show Ended Sessions"
  /// A session's menu's item that shows its work in Activity.
  static let showActivity = "Show Activity"
  /// A session's menu's item that shows its screenshots.
  static let showScreenshots = "Show Screenshots"
  /// The heading over a session's facts in its menu.
  static let details = "Details"
  /// The Helper menu's and the helper line's item that reads the helper's status now.
  static let checkAgainItem = "Check Again"
  /// The Helper menu's and the helper line's item that opens Settings on Helper.
  static let openSettingsItem = "Open Settings on Helper…"
  /// The View menu's item while the sidebar shows.
  static let hideSidebarItem = "Hide Sidebar"
  /// The View menu's item while the sidebar is hidden.
  static let showSidebarItem = "Show Sidebar"
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

  /// A place's word, in the Library and the View menu.
  ///
  /// - Parameter place: The place.
  /// - Returns: "Screen", "Activity", "Apps" or "Screenshots".
  static func place(_ place: ViewerPlace) -> String {
    switch place {
    case .screen: "Screen"
    case .activity: "Activity"
    case .apps: "Apps"
    case .screenshots: "Screenshots"
    }
  }
}
