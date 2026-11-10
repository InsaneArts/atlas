import AppKit
import AtlasStatus
import AtlasUI

/// The glyph's drawing geometry, in points of the menu bar's 24 × 22-point item, top down.
enum StatusGlyphLayout {
  /// 22.5: the orbit's width, the ring's outer reach, in a 22.5 × 13 box.
  static let orbit: CGFloat = 22.5
  /// 12, 11: the orbit's centre, the item's.
  static let orbitCentre = CGPoint(x: 12, y: 11)
  /// 1: the thinnest the orbit's line draws.
  static let orbitLeast: CGFloat = 1
  /// 17, 13, 7 × 7: the badge's box at the item's lower right, its right edge on the item's and its bottom 2 up from
  /// the item's bottom edge. A 7-point disc, ring or square, on whole pixels at one pixel a point.
  static let badge = CGRect(x: 17, y: 13, width: 7, height: 7)
  /// 2: the square's corners.
  static let squareCorner: CGFloat = 2
  /// 1: the gap knocked out of the glyph around the badge.
  static let ring: CGFloat = 1
  /// 1: a glyph shown whole.
  static let whole: CGFloat = 1
  /// 2: the pixels a point is taken to have when no context is current, a Retina display's.
  static let fallbackScale: CGFloat = 2
}
