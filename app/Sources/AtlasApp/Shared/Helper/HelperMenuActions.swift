import Foundation

/// What the helper's menu items ask, wherever they stand: the main menu's Helper menu, the main window's ⋯
/// and the panel's ⋯. The window that answers them implements each and validates its items.
@objc protocol HelperMenuActions {
  /// Recover…: ask before recovering the helper.
  ///
  /// - Parameter sender: The menu item.
  func askRecover(_ sender: Any?)
  /// Restart the Helper…: ask before restarting it.
  ///
  /// - Parameter sender: The menu item.
  func askRestart(_ sender: Any?)
  /// Switch to the Helper…: ask before switching this Mac's screen to it.
  ///
  /// - Parameter sender: The menu item.
  func askSwitch(_ sender: Any?)
  /// Log Out the Helper…: ask before logging it out.
  ///
  /// - Parameter sender: The menu item.
  func askLogOut(_ sender: Any?)
}
