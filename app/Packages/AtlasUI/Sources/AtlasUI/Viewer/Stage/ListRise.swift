import SwiftUI

/// How a list moves as it comes from the Screen or goes back to it. That is a change of depth, so the list rises
/// ``ViewerLayout/listRise`` into place as it fades in, and sinks the same way as it fades out, while the picture travels
/// between the stage and the corner. Its offset rides ``Motion/zoom``, the picture's spring, and its fade the panes'
/// own (``PaneShift``), so the picture leads. A step from one list to another is sideways and only crossfades, and
/// under Reduce Motion nothing rises either.
enum ListRise {
  /// Whether a switch from `old` to `new` moves a list in depth: one of them is the Screen and the other a list, and
  /// Reduce Motion is off.
  ///
  /// - Parameters:
  ///   - old: The place left.
  ///   - new: The place shown.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: Whether the list rises in or sinks out.
  static func rises(from old: ViewerPlace, to new: ViewerPlace, reduceMotion: Bool) -> Bool {
    guard !reduceMotion else { return false }
    return (old == .screen && new.isKept) || (new == .screen && old.isKept)
  }

  /// How far below its place a list draws: ``ViewerLayout/listRise`` while lowered, before it rises in or once it has
  /// sunk out; else in place.
  ///
  /// - Parameter isLowered: Whether it is lowered.
  /// - Returns: Its vertical offset.
  static func offset(isLowered: Bool) -> CGFloat {
    isLowered ? ViewerLayout.listRise : .zero
  }

  /// The list's rise and sink: ``Motion/zoom``, the spring the picture moves on.
  static var motion: Animation { Motion.zoom }
}
