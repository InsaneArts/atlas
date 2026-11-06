import AtlasUI

/// What a key means over the Activity pane while an action shows large or a task replays, as the window's key monitor
/// reads it. Shown large, ← and → step through the actions, B and A show the screen before and after, and Esc closes
/// it. Replaying, ← and → step through the frames, Space plays or pauses, and Esc closes it. Keys held with ⌘, ⌃ or ⌥
/// keep their own meaning.
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
  ///   - large: Whether an action shows large.
  ///   - replaying: Whether a task replays.
  /// - Returns: The event, or nil when the key means nothing here.
  static func event(keyCode: UInt16, hasModifiers: Bool, large: Bool, replaying: Bool) -> ViewerEvent? {
    guard !hasModifiers else { return nil }
    if replaying {
      switch keyCode {
      case ShotKeys.left: return .previousFrame
      case ShotKeys.right: return .nextFrame
      case ShotKeys.space: return .togglePlay
      case ShotKeys.escape: return .closeReplay
      default: return nil
      }
    }
    guard large else { return nil }
    switch keyCode {
    case ShotKeys.left: return .previousAction
    case ShotKeys.right: return .nextAction
    case before: return .compareAction(.before)
    case after: return .compareAction(.after)
    case ShotKeys.escape: return .closeAction
    default: return nil
    }
  }
}
