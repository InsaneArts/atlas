import CoreGraphics

/// Line widths. The hairline is half a point, one device pixel on a Retina display, so a card's edge stays fine.
enum Stroke {
  /// 0.5: a card's or a capsule's edge.
  static let hairline: CGFloat = 0.5
  /// 1: a selected row's edge.
  static let regular: CGFloat = 1
  /// 1.5: an unselected radio, an unchecked mark, and an edge that shows control.
  static let emphasis: CGFloat = 1.5
  /// 2: a spinner's arc.
  static let thick: CGFloat = 2
}
