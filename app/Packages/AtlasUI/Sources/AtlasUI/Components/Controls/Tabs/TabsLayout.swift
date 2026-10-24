import CoreGraphics

/// The measures the tabs' styles share, from v3's track (`.vtabs`) and tiles (G1).
enum TabsLayout {
  /// 28: a track tab's height, and the thumb's.
  static let trackHeight = ControlHeight.s
  /// 3: the track's inset around its tabs, so the track stands 34 high.
  static let trackInset: CGFloat = 3
  /// 2: between two track tabs.
  static let trackGap = Spacing.half
  /// 3: between a tile's symbol and its word.
  static let tileGap: CGFloat = 3
}
