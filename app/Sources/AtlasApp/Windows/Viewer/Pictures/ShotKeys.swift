import AtlasUI

/// What a key means over the sidebar shown over the card, on the Screenshots tab, and on the Screen, as the window's
/// key monitor reads it: Escape puts the sidebar over the card away. On the Screenshots tab, Space shows the selected
/// screenshot large or closes it, Return opens it, ← and → move along, ↑ and ↓ move a row, and Esc closes the large
/// view, or with none large, clears the selection. Keys held with ⌘, ⌃ or ⌥ keep their own meaning, so ⌘C copies the
/// selected one through the Edit menu.
enum ShotKeys {
  /// Space's key code.
  static let space: UInt16 = 49
  /// Return's key code.
  static let returnKey: UInt16 = 36
  /// ←'s key code.
  static let left: UInt16 = 123
  /// →'s key code.
  static let right: UInt16 = 124
  /// ↓'s key code.
  static let down: UInt16 = 125
  /// ↑'s key code.
  static let up: UInt16 = 126
  /// Esc's key code.
  static let escape: UInt16 = 53

  /// The event a key sends on the Screenshots tab.
  ///
  /// - Parameters:
  ///   - keyCode: The key.
  ///   - hasModifiers: Whether ⌘, ⌃ or ⌥ is held with it.
  ///   - largeShown: Whether a screenshot is shown large.
  /// - Returns: The event, or nil when the key means nothing here.
  static func event(keyCode: UInt16, hasModifiers: Bool, largeShown: Bool) -> ViewerEvent? {
    guard !hasModifiers else { return nil }
    switch keyCode {
    case space: return .toggleShot
    case returnKey: return largeShown ? nil : .toggleShot
    case left: return .previousShot
    case right: return .nextShot
    case up: return largeShown ? nil : .shotAbove
    case down: return largeShown ? nil : .shotBelow
    case escape: return largeShown ? .closeShot : .clearShotSelection
    default: return nil
    }
  }

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
