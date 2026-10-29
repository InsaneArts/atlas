import Foundation

/// What the main menu's View, Session, File, Edit and Helper items and the Dock menu's ask of the window in front. The main
/// window answers each and validates its item against the same state its buttons draw.
@objc protocol ViewerMenuActions {
  /// Screen, Activity, Screenshots or Apps, by the item's tag (``ViewerPlace/menuTag``).
  ///
  /// - Parameter sender: The menu item.
  func showPlace(_ sender: Any?)
  /// Show Sidebar or Hide Sidebar.
  ///
  /// - Parameter sender: The menu item.
  func toggleSidebar(_ sender: Any?)
  /// Pause Picture or Resume Picture.
  ///
  /// - Parameter sender: The menu item.
  func togglePicturePause(_ sender: Any?)
  /// Actual Size.
  ///
  /// - Parameter sender: The menu item.
  func toggleActualSize(_ sender: Any?)
  /// Take Control or Give Back Control.
  ///
  /// - Parameter sender: The menu item.
  func toggleControl(_ sender: Any?)
  /// Stop Agent or Resume Agent.
  ///
  /// - Parameter sender: The menu item.
  func toggleAgent(_ sender: Any?)
  /// Save Picture…, or Save Screenshot… in the Screenshots list.
  ///
  /// - Parameter sender: The menu item.
  func savePicture(_ sender: Any?)
  /// Live or Every 5 Seconds, by the item's tag (``ViewerPictureRate/menuTag``).
  ///
  /// - Parameter sender: The menu item.
  func chooseRate(_ sender: Any?)
  /// Find…: the sidebar's search takes the focus, on the Screen and on every list.
  ///
  /// - Parameter sender: The menu item.
  func findInTab(_ sender: Any?)
  /// One of Send Key's keys, by the item's tag (``HelperKey/all``): sent to the helper while this window holds control.
  ///
  /// - Parameter sender: The menu item.
  func sendKey(_ sender: Any?)
  /// Check Again: read the helper's status now.
  ///
  /// - Parameter sender: The menu item.
  func checkAgain(_ sender: Any?)
  /// Open Settings on Helper….
  ///
  /// - Parameter sender: The menu item.
  func openHelperSettings(_ sender: Any?)
  /// All Sessions: every session's work.
  ///
  /// - Parameter sender: The menu item.
  func chooseAllSessions(_ sender: Any?)
  /// Previous Session: the session over the one chosen, in the sidebar's order.
  ///
  /// - Parameter sender: The menu item.
  func choosePreviousSession(_ sender: Any?)
  /// Next Session: the session under the one chosen, in the sidebar's order.
  ///
  /// - Parameter sender: The menu item.
  func chooseNextSession(_ sender: Any?)
  /// Show Ended Sessions, ticked while Ended lists every session that ended.
  ///
  /// - Parameter sender: The menu item.
  func toggleEnded(_ sender: Any?)
  /// Clean Up This Session's Work…, for the session chosen.
  ///
  /// - Parameter sender: The menu item.
  func askCleanupChosen(_ sender: Any?)
}
