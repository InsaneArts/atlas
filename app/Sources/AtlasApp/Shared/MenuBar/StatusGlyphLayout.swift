import AppKit
import AtlasStatus
import AtlasUI

/// The glyph's drawing geometry, in points of the menu bar's 22-point item.
enum StatusGlyphLayout {
  /// 17.9: the orbit's width, the ring's outer reach, in an 18 × 10.3 box.
  static let orbit: CGFloat = 17.9
  /// 1: the thinnest the orbit's line draws.
  static let orbitLeast: CGFloat = 1
  /// 15, 13, 7 × 7: the badge's box in the item's square, top down, at the orbit's lower right: its centre 1.5 in from
  /// the orbit's right end, its bottom 2 up from the item's bottom edge. A 7-point disc, ring or square, on whole
  /// pixels at one pixel a point.
  static let badge = CGRect(x: 15, y: 13, width: 7, height: 7)
  /// 2: the square's corners.
  static let squareCorner: CGFloat = 2
  /// 1: the gap knocked out of the orbit around the badge.
  static let ring: CGFloat = 1
  /// 1: an orbit shown whole.
  static let whole: CGFloat = 1
  /// 2: the pixels a point is taken to have when no context is current, a Retina display's.
  static let fallbackScale: CGFloat = 2
}
