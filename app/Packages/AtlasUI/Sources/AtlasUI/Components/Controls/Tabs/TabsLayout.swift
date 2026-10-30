import SwiftUI

/// The measures the tabs' styles share.
enum TabsLayout {
  /// 24: a track tab's height, and the thumb's.
  static let trackHeight = ControlHeight.xs
  /// 2: the track's inset around its tabs, so the track stands 28 high, a search field's height, and the thumb's
  /// curve runs alongside the track's.
  static let trackInset = Spacing.half
  /// 2: between two track tabs.
  static let trackGap = Spacing.half
  /// 3: between a tile's symbol and its word.
  static let tileGap: CGFloat = 3
  /// 3 pt: stationary presses remain taps; drags preserve the entire translation once recognised.
  static let dragThreshold: CGFloat = 3
  /// 8 pt: the maximum resistance beyond either end of the track.
  static let rubberBandLimit = Spacing.s
  /// 2 pt: the keyboard focus ring's distance outside the track.
  static let focusGap = Spacing.half
}
