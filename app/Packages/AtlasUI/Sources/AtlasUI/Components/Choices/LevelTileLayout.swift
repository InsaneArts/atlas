import CoreGraphics

/// A level tile's own measures.
enum LevelTileLayout {
  /// 60: the tile's height.
  static let height: CGFloat = 60
  /// 0.8: the most a long word is shrunk to stay on one line under a narrow tile.
  static let wordShrink: CGFloat = 0.8
}
