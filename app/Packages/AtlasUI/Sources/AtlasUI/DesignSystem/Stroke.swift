import CoreGraphics

/// Line widths. The hairline is half a point, one device pixel on a Retina display, so an edge stays fine. Controls,
/// rows and cards draw no edge: they read by their fill.
enum Stroke {
  /// 0.5: a content pane's edge, a selected capsule's rim and a chip's edge over a picture.
  static let hairline: CGFloat = 0.5
  /// 1: a focus ring, a field's edge, a mark's ring and a chosen card's firmer edge; selection itself never strokes.
  static let regular: CGFloat = 1
  /// 1.5: an unselected radio, an unchecked mark, and an edge that shows control.
  static let emphasis: CGFloat = 1.5
  /// 2: a spinner's arc.
  static let thick: CGFloat = 2
}
