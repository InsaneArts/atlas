import AppKit

/// The Screen glyph's drawing geometry, in points of the menu bar's 24 × 22-point item, top down: a display and its
/// stand as tall as the menu bar's own icons, about 15 points, and the orbit on the display.
enum StatusScreenLayout {
  /// The display's outline, down the middle of its stroke.
  static let display = CGRect(x: 2.5, y: 3.5, width: 19, height: 12)
  /// 2.8: the display's corners.
  static let corner: CGFloat = 2.8
  /// 1.35: the display's stroke.
  static let stroke: CGFloat = 1.35
  /// 9.5 to 14.5, 18.2 down: the stand, a line with round ends.
  static let stand = (from: CGPoint(x: 9.5, y: 18.2), to: CGPoint(x: 14.5, y: 18.2))
  /// 1.5: the stand's stroke.
  static let standStroke: CGFloat = 1.5
  /// 14.4: the orbit's width on the display, clear of its stroke above and below.
  static let orbit: CGFloat = 14.4
  /// 12, 9.5: the orbit's centre, the display's.
  static let orbitCentre = CGPoint(x: 12, y: 9.5)
  /// 1.1: the thinnest the orbit's line draws on the display.
  static let orbitLeast: CGFloat = 1.1
}
