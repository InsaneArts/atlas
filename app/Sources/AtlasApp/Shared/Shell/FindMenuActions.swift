import Foundation

/// The single Edit → Find responder action, shared by searchable windows.
@objc protocol FindMenuActions {
  /// Focuses the key window's search field.
  ///
  /// - Parameter sender: The existing menu item.
  func findInTab(_ sender: Any?)
}
