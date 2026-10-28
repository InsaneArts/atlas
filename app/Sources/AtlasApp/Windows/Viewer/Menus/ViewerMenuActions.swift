import Foundation

/// What the main menu's View, File, Edit and Helper items, the Dock menu's and the main window's ⋯ menu ask of the
/// window in front. The main window answers each and validates its item against the same state its buttons draw.
@objc protocol ViewerMenuActions {
  /// Screen, Activity, Apps or Screenshots, by the item's tag (``ViewerTab/menuTag``).
  ///
  /// - Parameter sender: The menu item.
  func selectTab(_ sender: Any?)
  /// Show Details or Hide Details.
  ///
  /// - Parameter sender: The menu item.
  func toggleDetails(_ sender: Any?)
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
  /// Save Picture…, or Save Screenshot… on the Screenshots tab.
  ///
  /// - Parameter sender: The menu item.
  func savePicture(_ sender: Any?)
  /// Live or Every 5 Seconds, by the item's tag (``ViewerPictureRate/menuTag``).
  ///
  /// - Parameter sender: The menu item.
  func chooseRate(_ sender: Any?)
  /// Find…: the shown tab's search field takes the focus.
  ///
  /// - Parameter sender: The menu item.
  func findInTab(_ sender: Any?)
  /// All Sessions, or one host session, by the item's represented key: whose work the window shows.
  ///
  /// - Parameter sender: The menu item.
  func chooseSession(_ sender: Any?)
}
