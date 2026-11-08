import AtlasUI

/// What a key means over the sidebar shown over the card and on the Screen, as the window's key monitor reads it,
/// and the key codes the picture sheet's and the replay's keys share (``ActionKeys``): Escape puts the sidebar over
/// the card away, and on the Screen stops showing a window outlined there. Keys held with ⌘, ⌃ or ⌥ keep their own
/// meaning.
enum WindowKeys {
  /// Space's key code.
  static let space: UInt16 = 49
  /// ←'s key code.
  static let left: UInt16 = 123
  /// →'s key code.
  static let right: UInt16 = 124
  /// Esc's key code.
  static let escape: UInt16 = 53

  /// The event a key sends while the sidebar shows over the card: Escape puts it away, unless this window holds
  /// control, where every key is the helper's.
  ///
  /// - Parameters:
  ///   - keyCode: The key.
  ///   - hasModifiers: Whether ⌘, ⌃ or ⌥ is held with it.
  ///   - holdingControl: Whether this window holds control.
  /// - Returns: The event, or nil when the key means nothing here.
  static func overlayEvent(keyCode: UInt16, hasModifiers: Bool, holdingControl: Bool) -> ViewerEvent? {
    guard keyCode == escape, !hasModifiers, !holdingControl else { return nil }
    return .dismissSidebarOverlay
  }

  /// The event a key sends on the Screen: Escape stops showing the window outlined there, as its label's × does,
  /// unless this window holds control, where every key is the helper's.
  ///
  /// - Parameters:
  ///   - keyCode: The key.
  ///   - hasModifiers: Whether ⌘, ⌃ or ⌥ is held with it.
  ///   - holdingControl: Whether this window holds control.
  ///   - outlineShown: Whether a window is outlined on the screen.
  /// - Returns: The event, or nil when the key means nothing here.
  static func screenEvent(keyCode: UInt16, hasModifiers: Bool, holdingControl: Bool, outlineShown: Bool) -> ViewerEvent?
  {
    guard keyCode == escape, !hasModifiers, !holdingControl, outlineShown else { return nil }
    return .clearOutline
  }
}
