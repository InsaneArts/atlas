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
}
