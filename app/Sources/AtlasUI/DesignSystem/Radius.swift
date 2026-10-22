import CoreGraphics

/// Corner radii. Capsules use the `Capsule` shape, not a radius.
enum Radius {
  /// 6: a popup button, a menu row and a small thumbnail, such as an Activity line's screenshot.
  static let compact: CGFloat = 6
  /// 8: a symbol tile and a search field.
  static let small: CGFloat = 8
  /// 10: a small picture's frame, a card group and a small window's empty stage.
  static let medium: CGFloat = 10
  /// 12: a card, a row, a pane and a large picture's frame.
  static let large: CGFloat = 12
  /// 22: an alert's corners.
  static let alert: CGFloat = 22
}
