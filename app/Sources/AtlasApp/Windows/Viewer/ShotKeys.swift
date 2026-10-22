import AtlasUI

/// What a key means on the Screenshots tab, as the window's key monitor reads it: Space shows the selected
/// screenshot large or closes it, ← and → move along, and Esc closes the large view. Keys held with ⌘, ⌃ or ⌥ keep
/// their own meaning, and Esc means nothing here while no screenshot is large.
enum ShotKeys {
  /// Space's key code.
  static let space: UInt16 = 49
  /// ←'s key code.
  static let left: UInt16 = 123
  /// →'s key code.
  static let right: UInt16 = 124
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
    case left: return .previousShot
    case right: return .nextShot
    case escape: return largeShown ? .closeShot : nil
    default: return nil
    }
  }
}
