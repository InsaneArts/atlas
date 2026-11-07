import AtlasUI

/// What a key means over the Activity pane while the picture sheet shows or a run replays, as the window's key monitor
/// reads it. In the sheet, ← and → step through the run's pictures, B and A show the screen before and after a step,
/// and Esc closes it; Return presses its default button, Done. Replaying, ← and → step through the frames, Space plays
/// or pauses, and Esc closes it. Keys held with ⌘, ⌃ or ⌥ keep their own meaning.
enum ActionKeys {
  /// A's key code.
  static let after: UInt16 = 0
  /// B's key code.
  static let before: UInt16 = 11

  /// The event a key sends.
  ///
  /// - Parameters:
  ///   - keyCode: The key.
  ///   - hasModifiers: Whether ⌘, ⌃ or ⌥ is held with it.
  ///   - sheet: Whether the picture sheet shows.
  ///   - replaying: Whether a task replays.
  /// - Returns: The event, or nil when the key means nothing here.
  static func event(keyCode: UInt16, hasModifiers: Bool, sheet: Bool, replaying: Bool) -> ViewerEvent? {
    guard !hasModifiers else { return nil }
    if replaying {
      switch keyCode {
      case WindowKeys.left: return .previousFrame
      case WindowKeys.right: return .nextFrame
      case WindowKeys.space: return .togglePlay
      case WindowKeys.escape: return .closeReplay
      default: return nil
      }
    }
    guard sheet else { return nil }
    switch keyCode {
    case WindowKeys.left: return .previousPicture
    case WindowKeys.right: return .nextPicture
    case before: return .pictureSide(.before)
    case after: return .pictureSide(.after)
    case WindowKeys.escape: return .closePicture
    default: return nil
    }
  }
}
