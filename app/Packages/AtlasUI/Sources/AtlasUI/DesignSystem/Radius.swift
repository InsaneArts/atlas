import CoreGraphics

/// Corner radii, named by size, `small` to `extraLarge`, and the window's own. Capsules use the `Capsule` shape, not a
/// radius. A shape inside another takes its corner from ``nested(in:inset:)``, so the two curves run side by side.
enum Radius {
  /// 6: a small thumbnail, such as an Activity line's screenshot, and a keycap.
  static let extraSmall: CGFloat = 6
  /// 8: a symbol tile.
  static let small: CGFloat = 8
  /// 10: a small picture's frame, a card group and a small window's empty stage.
  static let medium: CGFloat = 10
  /// 12: a card and a large picture's frame.
  static let large: CGFloat = 12
  /// 26: a modal surface's corners, such as an alert's (matching macOS 26 alerts), and the menu bar panel's card.
  static let extraLarge: CGFloat = 26
  /// 16: a window's corner as macOS 26 draws it, which the viewer's panes nest in. The system offers no way to read
  /// it, so it is the value it draws.
  static let window: CGFloat = 16

  /// The corner of a shape set `inset` inside one with `outer` corners, so both curves share a centre: the outer
  /// radius less the inset, and never below square.
  ///
  /// - Parameters:
  ///   - outer: The corner of the shape around it.
  ///   - inset: How far inside that shape's edge it sits.
  /// - Returns: The inner corner.
  static func nested(in outer: CGFloat, inset: CGFloat) -> CGFloat {
    max(outer - inset, .zero)
  }
}
