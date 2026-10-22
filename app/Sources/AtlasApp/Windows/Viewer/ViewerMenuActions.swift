import Foundation

/// What the main menu's View and File items, and the main window's ⋯ menu, ask of the window in front. The main
/// window answers each and validates its item against the same state its buttons draw.
@MainActor
@objc protocol ViewerMenuActions {
  /// Screen, Activity, Apps or Screenshots, by the item's tag (``ViewerMenuTag``).
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
  /// Take Control or Give Back.
  ///
  /// - Parameter sender: The menu item.
  func toggleControl(_ sender: Any?)
  /// Stop Agent or Resume Agent.
  ///
  /// - Parameter sender: The menu item.
  func toggleAgent(_ sender: Any?)
  /// Save Picture….
  ///
  /// - Parameter sender: The menu item.
  func savePicture(_ sender: Any?)
  /// Live, Every 5 seconds or Paused, by the item's tag (``ViewerPictureChoice``).
  ///
  /// - Parameter sender: The menu item.
  func choosePicture(_ sender: Any?)
}
