import SwiftUI

/// The measures and the motion the tabs' styles share.
enum TabsLayout {
  /// 28: a track tab's height, and the thumb's.
  static let trackHeight = ControlHeight.s
  /// 3: the track's inset around its tabs, so the track stands 34 high.
  static let trackInset: CGFloat = 3
  /// 2: between two track tabs.
  static let trackGap = Spacing.half
  /// 3: between a tile's symbol and its word.
  static let tileGap: CGFloat = 3

  /// How the thumb moves to the tab chosen: ``Motion/tab``, 0.25 seconds with no overshoot, or at once under Reduce
  /// Motion, where a glide is still movement.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The glide, or nil to jump.
  static func thumbGlide(reduceMotion: Bool) -> Animation? {
    Motion.animation(Motion.tab, reduceMotion: reduceMotion)
  }
}
